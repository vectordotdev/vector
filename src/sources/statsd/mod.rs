use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};
use vector_lib::ipallowlist::IpAllowlistConfig;

use bytes::Bytes;
use futures::{StreamExt, TryFutureExt};
use listenfd::ListenFd;
use serde_with::serde_as;
use smallvec::{smallvec, SmallVec};
use tokio_util::udp::UdpFramed;
use vector_lib::codecs::{
    decoding::{self, Deserializer, Framer},
    NewlineDelimitedDecoder,
};
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{CountByteSize, InternalEventHandle as _, Registered};
use vector_lib::EstimatedJsonEncodedSizeOf;

use self::parser::ParseError;
use super::util::net::{try_bind_udp_socket, SocketListenAddr, TcpNullAcker, TcpSource};
use crate::{
    codecs::Decoder,
    config::{GenerateConfig, Resource, SourceConfig, SourceContext, SourceOutput},
    event::Event,
    internal_events::{
        EventsReceived, SocketBindError, SocketBytesReceived, SocketMode, SocketReceiveError,
        StreamClosedError,
    },
    net,
    shutdown::ShutdownSignal,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsSourceConfig},
    SourceSender,
};

pub mod parser;
#[cfg(unix)]
mod unix;

use parser::Parser;

#[cfg(unix)]
use unix::{statsd_unix, UnixConfig};
use vector_lib::config::LogNamespace;

/// Configuration for the `statsd` source.
#[configurable_component(source("statsd", "Collect metrics emitted by the StatsD aggregator."))]
#[derive(Clone, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of socket to use."))]
#[allow(clippy::large_enum_variant)] // just used for configuration
pub enum StatsdConfig {
    /// Listen on TCP.
    Tcp(TcpConfig),

    /// Listen on UDP.
    Udp(UdpConfig),

    /// Listen on a Unix domain Socket (UDS).
    #[cfg(unix)]
    Unix(UnixConfig),
}

/// UDP configuration for the `statsd` source.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UdpConfig {
    #[configurable(derived)]
    address: SocketListenAddr,

    /// The size of the receive buffer used for each connection.
    receive_buffer_bytes: Option<usize>,

    #[serde(default = "default_sanitize")]
    #[configurable(derived)]
    sanitize: bool,
}

impl UdpConfig {
    pub const fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            receive_buffer_bytes: None,
            sanitize: default_sanitize(),
        }
    }
}

/// TCP configuration for the `statsd` source.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug)]
pub struct TcpConfig {
    #[configurable(derived)]
    address: SocketListenAddr,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    #[configurable(derived)]
    pub permit_origin: Option<IpAllowlistConfig>,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsSourceConfig>,

    /// The timeout before a connection is forcefully closed during shutdown.
    #[serde(default = "default_shutdown_timeout_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Shutdown Timeout"))]
    shutdown_timeout_secs: Duration,

    /// The size of the receive buffer used for each connection.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    receive_buffer_bytes: Option<usize>,

    /// The maximum number of TCP connections that are allowed at any given time.
    #[configurable(metadata(docs::type_unit = "connections"))]
    connection_limit: Option<u32>,

    ///	Whether or not to sanitize incoming statsd key names. When "true", keys are sanitized by:
    /// - "/" is replaced with "-"
    /// - All whitespace is replaced with "_"
    /// - All non alphanumeric characters [^a-zA-Z_\-0-9\.] are removed.
    #[serde(default = "default_sanitize")]
    #[configurable(derived)]
    sanitize: bool,
}

impl TcpConfig {
    #[cfg(test)]
    pub const fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            keepalive: None,
            permit_origin: None,
            tls: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            receive_buffer_bytes: None,
            connection_limit: None,
            sanitize: default_sanitize(),
        }
    }
}

const fn default_shutdown_timeout_secs() -> Duration {
    Duration::from_secs(30)
}

const fn default_sanitize() -> bool {
    true
}

impl GenerateConfig for StatsdConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::Udp(UdpConfig::from_address(
            SocketListenAddr::SocketAddr(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::LOCALHOST,
                8125,
            ))),
        )))
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "statsd")]
impl SourceConfig for StatsdConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        match self {
            StatsdConfig::Udp(config) => {
                Ok(Box::pin(statsd_udp(config.clone(), cx.shutdown, cx.out)))
            }
            StatsdConfig::Tcp(config) => {
                let tls_config = config.tls.as_ref().map(|tls| tls.tls_config.clone());
                let tls_client_metadata_key = config
                    .tls
                    .as_ref()
                    .and_then(|tls| tls.client_metadata_key.clone())
                    .and_then(|k| k.path);
                let tls = MaybeTlsSettings::from_config(&tls_config, true)?;
                let statsd_tcp_source = StatsdTcpSource {
                    sanitize: config.sanitize,
                };

                statsd_tcp_source.run(
                    config.address,
                    config.keepalive,
                    config.shutdown_timeout_secs,
                    tls,
                    tls_client_metadata_key,
                    config.receive_buffer_bytes,
                    None,
                    cx,
                    false.into(),
                    config.connection_limit,
                    config.permit_origin.clone().map(Into::into),
                    StatsdConfig::NAME,
                    LogNamespace::Legacy,
                )
            }
            #[cfg(unix)]
            StatsdConfig::Unix(config) => statsd_unix(config.clone(), cx.shutdown, cx.out),
        }
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn resources(&self) -> Vec<Resource> {
        match self.clone() {
            Self::Tcp(tcp) => vec![tcp.address.as_tcp_resource()],
            Self::Udp(udp) => vec![udp.address.as_udp_resource()],
            #[cfg(unix)]
            Self::Unix(_) => vec![],
        }
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub(crate) struct StatsdDeserializer {
    socket_mode: Option<SocketMode>,
    events_received: Option<Registered<EventsReceived>>,
    parser: Parser,
}

impl StatsdDeserializer {
    pub fn udp(sanitize: bool) -> Self {
        Self {
            socket_mode: Some(SocketMode::Udp),
            // The other modes emit a different `EventsReceived`.
            events_received: Some(register!(EventsReceived)),
            parser: Parser::new(sanitize),
        }
    }

    pub const fn tcp(sanitize: bool) -> Self {
        Self {
            socket_mode: None,
            events_received: None,
            parser: Parser::new(sanitize),
        }
    }

    #[cfg(unix)]
    pub const fn unix(sanitize: bool) -> Self {
        Self {
            socket_mode: Some(SocketMode::Unix),
            events_received: None,
            parser: Parser::new(sanitize),
        }
    }
}

impl decoding::format::Deserializer for StatsdDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        _log_namespace: LogNamespace,
    ) -> crate::Result<SmallVec<[Event; 1]>> {
        // The other modes already emit BytesReceived
        if let Some(mode) = self.socket_mode {
            if mode == SocketMode::Udp {
                emit!(SocketBytesReceived {
                    mode,
                    byte_size: bytes.len(),
                });
            }
        }

        match std::str::from_utf8(&bytes).map_err(ParseError::InvalidUtf8) {
            Err(error) => Err(Box::new(error)),
            Ok(s) => match self.parser.parse(s) {
                Ok(metric) => {
                    let event = Event::Metric(metric);
                    if let Some(er) = &self.events_received {
                        let byte_size = event.estimated_json_encoded_size_of();
                        er.emit(CountByteSize(1, byte_size));
                    }
                    Ok(smallvec![event])
                }
                Err(error) => Err(Box::new(error)),
            },
        }
    }
}

async fn statsd_udp(
    config: UdpConfig,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let listenfd = ListenFd::from_env();
    let socket = try_bind_udp_socket(config.address, listenfd)
        .map_err(|error| {
            emit!(SocketBindError {
                mode: SocketMode::Udp,
                error
            })
        })
        .await?;

    if let Some(receive_buffer_bytes) = config.receive_buffer_bytes {
        if let Err(error) = net::set_receive_buffer_size(&socket, receive_buffer_bytes) {
            warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
        }
    }

    info!(
        message = "Listening.",
        addr = %config.address,
        r#type = "udp"
    );

    let codec = Decoder::new(
        Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
        Deserializer::Boxed(Box::new(StatsdDeserializer::udp(config.sanitize))),
    );
    let mut stream = UdpFramed::new(socket, codec).take_until(shutdown);
    while let Some(frame) = stream.next().await {
        match frame {
            Ok(((events, _byte_size), _sock)) => {
                let count = events.len();
                if (out.send_batch(events).await).is_err() {
                    emit!(StreamClosedError { count });
                }
            }
            Err(error) => {
                emit!(SocketReceiveError {
                    mode: SocketMode::Udp,
                    error
                });
            }
        }
    }

    Ok(())
}

#[derive(Clone)]
struct StatsdTcpSource {
    sanitize: bool,
}

impl TcpSource for StatsdTcpSource {
    type Error = vector_lib::codecs::decoding::Error;
    type Item = SmallVec<[Event; 1]>;
    type Decoder = Decoder;
    type Acker = TcpNullAcker;

    fn decoder(&self) -> Self::Decoder {
        Decoder::new(
            Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
            Deserializer::Boxed(Box::new(StatsdDeserializer::tcp(self.sanitize))),
        )
    }

    fn build_acker(&self, _: &[Self::Item]) -> Self::Acker {
        TcpNullAcker
    }
}

#[cfg(test)]
mod test {
    use futures::channel::mpsc;
    use futures_util::SinkExt;
    use tokio::{
        io::AsyncWriteExt,
        net::UdpSocket,
        time::{sleep, Duration, Instant},
    };
    use vector_lib::{
        config::ComponentKey,
        event::{metric::TagValue, EventContainer},
    };

    use super::*;
    use crate::test_util::{
        collect_limited,
        components::{
            assert_source_compliance, assert_source_error, COMPONENT_ERROR_TAGS,
            SOCKET_PUSH_SOURCE_TAGS,
        },
        metrics::{assert_counter, assert_distribution, assert_gauge, assert_set},
        next_addr,
    };
    use crate::{series, test_util::metrics::AbsoluteMetricState};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StatsdConfig>();
    }

    #[tokio::test]
    async fn test_statsd_udp() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async move {
            let in_addr = next_addr();
            let config = StatsdConfig::Udp(UdpConfig::from_address(in_addr.into()));
            let (sender, mut receiver) = mpsc::channel(200);
            tokio::spawn(async move {
                let bind_addr = next_addr();
                let socket = UdpSocket::bind(bind_addr).await.unwrap();
                socket.connect(in_addr).await.unwrap();
                while let Some(bytes) = receiver.next().await {
                    socket.send(bytes).await.unwrap();
                }
            });
            test_statsd(config, sender).await;
        })
        .await;
    }

    #[tokio::test]
    async fn test_statsd_tcp() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async move {
            let in_addr = next_addr();
            let config = StatsdConfig::Tcp(TcpConfig::from_address(in_addr.into()));
            let (sender, mut receiver) = mpsc::channel(200);
            tokio::spawn(async move {
                while let Some(bytes) = receiver.next().await {
                    tokio::net::TcpStream::connect(in_addr)
                        .await
                        .unwrap()
                        .write_all(bytes)
                        .await
                        .unwrap();
                }
            });
            test_statsd(config, sender).await;
        })
        .await;
    }

    #[tokio::test]
    async fn test_statsd_error() {
        assert_source_error(&COMPONENT_ERROR_TAGS, async move {
            let in_addr = next_addr();
            let config = StatsdConfig::Tcp(TcpConfig::from_address(in_addr.into()));
            let (sender, mut receiver) = mpsc::channel(200);
            tokio::spawn(async move {
                while let Some(bytes) = receiver.next().await {
                    tokio::net::TcpStream::connect(in_addr)
                        .await
                        .unwrap()
                        .write_all(bytes)
                        .await
                        .unwrap();
                }
            });
            test_invalid_statsd(config, sender).await;
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_statsd_unix() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async move {
            let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");
            let config = StatsdConfig::Unix(UnixConfig {
                path: in_path.clone(),
                sanitize: true,
            });
            let (sender, mut receiver) = mpsc::channel(200);
            tokio::spawn(async move {
                while let Some(bytes) = receiver.next().await {
                    tokio::net::UnixStream::connect(&in_path)
                        .await
                        .unwrap()
                        .write_all(bytes)
                        .await
                        .unwrap();
                }
            });
            test_statsd(config, sender).await;
        })
        .await;
    }

    async fn test_statsd(statsd_config: StatsdConfig, mut sender: mpsc::Sender<&'static [u8]>) {
        // Build our statsd source and then spawn it.  We use a big pipeline buffer because each
        // packet we send has a lot of metrics per packet.  We could technically count them all up
        // and have a more accurate number here, but honestly, who cares?  This is big enough.
        let component_key = ComponentKey::from("statsd");
        let (tx, rx) = SourceSender::new_test_sender_with_buffer(4096);
        let (source_ctx, shutdown) = SourceContext::new_shutdown(&component_key, tx);
        let sink = statsd_config
            .build(source_ctx)
            .await
            .expect("failed to build statsd source");

        tokio::spawn(async move {
            sink.await.expect("sink should not fail");
        });

        // Wait like 250ms to give the sink time to start running and become ready to handle
        // traffic.
        //
        // TODO: It'd be neat if we could make `ShutdownSignal` track when it was polled at least once,
        // and then surface that (via one of the related types, maybe) somehow so we could use it as
        // a signal for "the sink is ready, it's polled the shutdown future at least once, which
        // means it's trying to accept connections, etc" and would be far more deterministic than this.
        sleep(Duration::from_millis(250)).await;

        // Send all of the messages.
        for _ in 0..100 {
            sender.send(
                b"foo:1|c|#a,b:b\nbar:42|g\nfoo:1|c|#a,b:c\nglork:3|h|@0.1\nmilliglork:3000|ms|@0.2\nset:0|s\nset:1|s\n"
            ).await.unwrap();

            // Space things out slightly to try to avoid dropped packets.
            sleep(Duration::from_millis(10)).await;
        }

        // Now wait for another small period of time to make sure we've processed the messages.
        // After that, trigger shutdown so our source closes and allows us to deterministically read
        // everything that was in up without having to know the exact count.
        sleep(Duration::from_millis(250)).await;
        shutdown
            .shutdown_all(Some(Instant::now() + Duration::from_millis(100)))
            .await;

        // Read all the events into a `MetricState`, which handles normalizing metrics and tracking
        // cumulative values for incremental metrics, etc.  This will represent the final/cumulative
        // values for each metric sent by the source into the pipeline.
        let state = collect_limited(rx)
            .await
            .into_iter()
            .flat_map(EventContainer::into_events)
            .collect::<AbsoluteMetricState>();
        let metrics = state.finish();

        assert_counter(
            &metrics,
            series!(
                "foo",
                "a" => TagValue::Bare,
                "b" => "b"
            ),
            100.0,
        );

        assert_counter(
            &metrics,
            series!(
                "foo",
                "a" => TagValue::Bare,
                "b" => "c"
            ),
            100.0,
        );

        assert_gauge(&metrics, series!("bar"), 42.0);
        assert_distribution(
            &metrics,
            series!("glork"),
            3000.0,
            1000,
            &[(1.0, 0), (2.0, 0), (4.0, 1000), (f64::INFINITY, 1000)],
        );
        assert_distribution(
            &metrics,
            series!("milliglork"),
            1500.0,
            500,
            &[(1.0, 0), (2.0, 0), (4.0, 500), (f64::INFINITY, 500)],
        );
        assert_set(&metrics, series!("set"), &["0", "1"]);
    }

    async fn test_invalid_statsd(
        statsd_config: StatsdConfig,
        mut sender: mpsc::Sender<&'static [u8]>,
    ) {
        // Build our statsd source and then spawn it.  We use a big pipeline buffer because each
        // packet we send has a lot of metrics per packet.  We could technically count them all up
        // and have a more accurate number here, but honestly, who cares?  This is big enough.
        let component_key = ComponentKey::from("statsd");
        let (tx, _rx) = SourceSender::new_test_sender_with_buffer(4096);
        let (source_ctx, shutdown) = SourceContext::new_shutdown(&component_key, tx);
        let sink = statsd_config
            .build(source_ctx)
            .await
            .expect("failed to build statsd source");

        tokio::spawn(async move {
            sink.await.expect("sink should not fail");
        });

        // Wait like 250ms to give the sink time to start running and become ready to handle
        // traffic.
        //
        // TODO: It'd be neat if we could make `ShutdownSignal` track when it was polled at least once,
        // and then surface that (via one of the related types, maybe) somehow so we could use it as
        // a signal for "the sink is ready, it's polled the shutdown future at least once, which
        // means it's trying to accept connections, etc" and would be far more deterministic than this.
        sleep(Duration::from_millis(250)).await;

        // Send 10 invalid statsd messages
        for _ in 0..10 {
            sender.send(b"invalid statsd message").await.unwrap();

            // Space things out slightly to try to avoid dropped packets.
            sleep(Duration::from_millis(10)).await;
        }

        // Now wait for another small period of time to make sure we've processed the messages.
        // After that, trigger shutdown so our source closes and allows us to deterministically read
        // everything that was in up without having to know the exact count.
        sleep(Duration::from_millis(250)).await;
        shutdown
            .shutdown_all(Some(Instant::now() + Duration::from_millis(100)))
            .await;
    }
}
