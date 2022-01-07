use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use bytes::Bytes;
use futures::{StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use tokio::net::UdpSocket;
use tokio_util::udp::UdpFramed;

use self::parser::ParseError;
use super::util::{SocketListenAddr, TcpNullAcker, TcpSource};
use crate::{
    codecs::{self, decoding::Deserializer, NewlineDelimitedDecoder},
    config::{
        self, GenerateConfig, Output, Resource, SourceConfig, SourceContext, SourceDescription,
    },
    event::Event,
    internal_events::{StatsdEventReceived, StatsdInvalidRecord, StatsdSocketError},
    shutdown::ShutdownSignal,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
    udp, SourceSender,
};

pub mod parser;
#[cfg(unix)]
mod unix;

use parser::parse;
#[cfg(unix)]
use unix::{statsd_unix, UnixConfig};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum StatsdConfig {
    Tcp(TcpConfig),
    Udp(UdpConfig),
    #[cfg(unix)]
    Unix(UnixConfig),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UdpConfig {
    address: SocketAddr,
    receive_buffer_bytes: Option<usize>,
}

impl UdpConfig {
    pub const fn from_address(address: SocketAddr) -> Self {
        Self {
            address,
            receive_buffer_bytes: None,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct TcpConfig {
    address: SocketListenAddr,
    keepalive: Option<TcpKeepaliveConfig>,
    #[serde(default)]
    tls: Option<TlsConfig>,
    #[serde(default = "default_shutdown_timeout_secs")]
    shutdown_timeout_secs: u64,
    receive_buffer_bytes: Option<usize>,
    connection_limit: Option<u32>,
}

impl TcpConfig {
    #[cfg(test)]
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            keepalive: None,
            tls: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            receive_buffer_bytes: None,
            connection_limit: None,
        }
    }
}

const fn default_shutdown_timeout_secs() -> u64 {
    30
}

inventory::submit! {
    SourceDescription::new::<StatsdConfig>("statsd")
}

impl GenerateConfig for StatsdConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::Udp(UdpConfig::from_address(SocketAddr::V4(
            SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8125),
        ))))
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
                let tls = MaybeTlsSettings::from_config(&config.tls, true)?;
                StatsdTcpSource.run(
                    config.address,
                    config.keepalive,
                    config.shutdown_timeout_secs,
                    tls,
                    config.receive_buffer_bytes,
                    cx,
                    false.into(),
                    config.connection_limit,
                )
            }
            #[cfg(unix)]
            StatsdConfig::Unix(config) => Ok(statsd_unix(config.clone(), cx.shutdown, cx.out)),
        }
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(config::DataType::Metric)]
    }

    fn source_type(&self) -> &'static str {
        "statsd"
    }

    fn resources(&self) -> Vec<Resource> {
        match self.clone() {
            Self::Tcp(tcp) => vec![tcp.address.into()],
            Self::Udp(udp) => vec![Resource::udp(udp.address)],
            #[cfg(unix)]
            Self::Unix(_) => vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatsdDeserializer;

impl Deserializer for StatsdDeserializer {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        match std::str::from_utf8(&bytes)
            .map_err(ParseError::InvalidUtf8)
            .and_then(parse)
        {
            Ok(metric) => {
                emit!(&StatsdEventReceived {
                    byte_size: bytes.len()
                });
                Ok(smallvec![Event::Metric(metric)])
            }
            Err(error) => {
                emit!(&StatsdInvalidRecord {
                    error: &error,
                    bytes
                });
                Err(Box::new(error))
            }
        }
    }
}

async fn statsd_udp(
    config: UdpConfig,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let socket = UdpSocket::bind(&config.address)
        .map_err(|error| emit!(&StatsdSocketError::bind(error)))
        .await?;

    if let Some(receive_buffer_bytes) = config.receive_buffer_bytes {
        if let Err(error) = udp::set_receive_buffer_size(&socket, receive_buffer_bytes) {
            warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
        }
    }

    info!(
        message = "Listening.",
        addr = %config.address,
        r#type = "udp"
    );

    let codec = codecs::Decoder::new(
        Box::new(NewlineDelimitedDecoder::new()),
        Box::new(StatsdDeserializer),
    );
    let mut stream = UdpFramed::new(socket, codec).take_until(shutdown);
    while let Some(frame) = stream.next().await {
        match frame {
            Ok(((events, _byte_size), _sock)) => {
                for metric in events {
                    if let Err(error) = out.send(metric).await {
                        error!(message = "Error sending metric.", %error);
                        break;
                    }
                }
            }
            Err(error) => {
                emit!(&StatsdSocketError::read(error));
            }
        }
    }

    Ok(())
}

#[derive(Clone)]
struct StatsdTcpSource;

impl TcpSource for StatsdTcpSource {
    type Error = codecs::decoding::Error;
    type Item = SmallVec<[Event; 1]>;
    type Decoder = codecs::Decoder;
    type Acker = TcpNullAcker;

    fn decoder(&self) -> Self::Decoder {
        codecs::Decoder::new(
            Box::new(NewlineDelimitedDecoder::new()),
            Box::new(StatsdDeserializer),
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
        time::{sleep, Duration, Instant},
    };
    use vector_core::config::ComponentKey;

    use super::*;
    use crate::series;
    use crate::test_util::{
        metrics::{assert_counter, assert_distribution, assert_gauge, assert_set, MetricState},
        next_addr,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StatsdConfig>();
    }

    #[tokio::test]
    async fn test_statsd_udp() {
        let in_addr = next_addr();
        let config = StatsdConfig::Udp(UdpConfig::from_address(in_addr));
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
    }

    #[tokio::test]
    async fn test_statsd_tcp() {
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
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_statsd_unix() {
        let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");
        let config = StatsdConfig::Unix(UnixConfig {
            path: in_path.clone(),
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
    }

    async fn test_statsd(statsd_config: StatsdConfig, mut sender: mpsc::Sender<&'static [u8]>) {
        // Build our statsd source and then spawn it.  We use a big pipeline buffer because each
        // packet we send has a lot of metrics per packet.  We could technically count them all up
        // and have a more accurate number here, but honestly, who cares?  This is big enough.
        let component_key = ComponentKey::from("statsd");
        let (tx, rx) = SourceSender::new_with_buffer(4096);
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
            .shutdown_all(Instant::now() + Duration::from_millis(100))
            .await;

        // Read all the events into a `MetricState`, which handles normalizing metrics and tracking
        // cumulative values for incremental metrics, etc.  This will represent the final/cumulative
        // values for each metric sent by the source into the pipeline.
        let state = rx.collect::<MetricState>().await;
        let metrics = state.finish();

        assert_counter(&metrics, series!("foo", "a" => "true", "b" => "b"), 100.0);
        assert_counter(&metrics, series!("foo", "a" => "true", "b" => "c"), 100.0);
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
}
