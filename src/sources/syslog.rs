#[cfg(unix)]
use std::path::PathBuf;
use std::{net::SocketAddr, num::NonZeroU64, time::Duration};

use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use listenfd::ListenFd;
use smallvec::SmallVec;
use tokio_util::udp::UdpFramed;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        BytesDecoder, OctetCountingDecoder, SyslogDeserializerConfig,
        decoding::{Deserializer, Framer},
    },
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol},
    ipallowlist::IpAllowlistConfig,
    lookup::{OwnedValuePath, lookup_v2::OptionalValuePath, path},
};
use vrl::event_path;

#[cfg(unix)]
use crate::sources::util::build_unix_stream_source;
use crate::{
    SourceSender,
    codecs::Decoder,
    config::{
        DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceOutput, log_schema,
    },
    event::Event,
    internal_events::{
        SocketBindError, SocketEventsReceived, SocketMode, SocketReceiveError, StreamClosedError,
    },
    net,
    shutdown::ShutdownSignal,
    sources::util::net::{SocketListenAddr, TcpNullAcker, TcpSource, try_bind_udp_socket},
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsSourceConfig},
};

/// Configuration for the `syslog` source.
#[configurable_component(source("syslog", "Collect logs sent via Syslog."))]
#[derive(Clone, Debug)]
pub struct SyslogConfig {
    #[serde(flatten)]
    mode: Mode,

    /// The maximum buffer size of incoming messages, in bytes.
    ///
    /// Messages larger than this are truncated.
    #[serde(default = "crate::serde::default_max_length")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    max_length: usize,

    /// Overrides the name of the log field used to add the peer host to each event.
    ///
    /// If using TCP or UDP, the value is the peer host's address, including the port. For example, `1.2.3.4:9000`. If using
    /// UDS, the value is the socket path itself.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    host_key: Option<OptionalValuePath>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

/// Listener mode for the `syslog` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of socket to use."))]
#[allow(clippy::large_enum_variant)]
pub enum Mode {
    /// Listen on TCP.
    Tcp {
        #[configurable(derived)]
        address: SocketListenAddr,

        #[configurable(derived)]
        keepalive: Option<TcpKeepaliveConfig>,

        #[configurable(derived)]
        permit_origin: Option<IpAllowlistConfig>,

        #[configurable(derived)]
        tls: Option<TlsSourceConfig>,

        /// The size of the receive buffer used for each connection.
        ///
        /// This should not typically needed to be changed.
        #[configurable(metadata(docs::type_unit = "bytes"))]
        receive_buffer_bytes: Option<usize>,

        /// The maximum number of TCP connections that are allowed at any given time.
        connection_limit: Option<u32>,

        /// The timeout, in seconds, before a TLS handshake is aborted if it has not completed.
        ///
        /// This bounds how long a connection can hold its slot against `connection_limit`
        /// before the TLS handshake finishes, protecting against clients that open a
        /// connection but never complete (or never start) a handshake.
        #[configurable(metadata(docs::type_unit = "seconds"))]
        tls_handshake_timeout_secs: Option<NonZeroU64>,
    },

    /// Listen on UDP.
    Udp {
        #[configurable(derived)]
        address: SocketListenAddr,

        /// The size of the receive buffer used for the listening socket.
        ///
        /// This should not typically needed to be changed.
        #[configurable(metadata(docs::type_unit = "bytes"))]
        receive_buffer_bytes: Option<usize>,
    },

    /// Listen on UDS (Unix domain socket). This only supports Unix stream sockets.
    ///
    /// For Unix datagram sockets, use the `socket` source instead.
    #[cfg(unix)]
    Unix {
        /// The Unix socket path.
        ///
        /// This should be an absolute path.
        #[configurable(metadata(docs::examples = "/path/to/socket"))]
        path: PathBuf,

        /// Unix file mode bits to be applied to the unix socket file as its designated file permissions.
        ///
        /// The file mode value can be specified in any numeric format supported by your configuration
        /// language, but it is most intuitive to use an octal number.
        socket_file_mode: Option<u32>,
    },
}

impl SyslogConfig {
    #[cfg(test)]
    pub fn from_mode(mode: Mode) -> Self {
        Self {
            mode,
            host_key: None,
            max_length: crate::serde::default_max_length(),
            log_namespace: None,
        }
    }
}

impl Default for SyslogConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Tcp {
                address: SocketListenAddr::SocketAddr("0.0.0.0:514".parse().unwrap()),
                keepalive: None,
                permit_origin: None,
                tls: None,
                receive_buffer_bytes: None,
                connection_limit: None,
                tls_handshake_timeout_secs: None,
            },
            host_key: None,
            max_length: crate::serde::default_max_length(),
            log_namespace: None,
        }
    }
}

impl GenerateConfig for SyslogConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(SyslogConfig::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "syslog")]
impl SourceConfig for SyslogConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let host_key = self
            .host_key
            .clone()
            .and_then(|k| k.path)
            .or(log_schema().host_key().cloned());

        match self.mode.clone() {
            Mode::Tcp {
                address,
                keepalive,
                permit_origin,
                tls,
                receive_buffer_bytes,
                connection_limit,
                tls_handshake_timeout_secs,
            } => {
                let source = SyslogTcpSource {
                    max_length: self.max_length,
                    host_key,
                    log_namespace,
                };
                let shutdown_secs = Duration::from_secs(30);
                let tls_config = tls.as_ref().map(|tls| tls.tls_config.clone());
                let tls_client_metadata_key = tls
                    .as_ref()
                    .and_then(|tls| tls.client_metadata_key.clone())
                    .and_then(|k| k.path);
                let tls = MaybeTlsSettings::from_config(tls_config.as_ref(), true)?;
                source.run(
                    address,
                    keepalive,
                    shutdown_secs,
                    tls,
                    None, // tls_reloader: not wired for this source
                    tls_client_metadata_key,
                    receive_buffer_bytes,
                    None,
                    tls_handshake_timeout_secs,
                    cx,
                    false.into(),
                    connection_limit,
                    permit_origin.map(Into::into),
                    SyslogConfig::NAME,
                    log_namespace,
                )
            }
            Mode::Udp {
                address,
                receive_buffer_bytes,
            } => Ok(udp(
                address,
                self.max_length,
                host_key,
                receive_buffer_bytes,
                cx.shutdown,
                log_namespace,
                cx.out,
            )),
            #[cfg(unix)]
            Mode::Unix {
                path,
                socket_file_mode,
            } => {
                let decoder = Decoder::new(
                    Framer::OctetCounting(OctetCountingDecoder::new_with_max_length(
                        self.max_length,
                    )),
                    Deserializer::Syslog(
                        SyslogDeserializerConfig::from_source(SyslogConfig::NAME).build(),
                    ),
                );

                build_unix_stream_source(
                    path,
                    socket_file_mode,
                    decoder,
                    move |events, host| handle_events(events, &host_key, host, log_namespace),
                    cx.shutdown,
                    cx.out,
                )
            }
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = SyslogDeserializerConfig::from_source(SyslogConfig::NAME)
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        match self.mode.clone() {
            Mode::Tcp { address, .. } => vec![address.as_tcp_resource()],
            Mode::Udp { address, .. } => vec![address.as_udp_resource()],
            #[cfg(unix)]
            Mode::Unix { .. } => vec![],
        }
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
struct SyslogTcpSource {
    max_length: usize,
    host_key: Option<OwnedValuePath>,
    log_namespace: LogNamespace,
}

impl TcpSource for SyslogTcpSource {
    type Error = vector_lib::codecs::decoding::Error;
    type Item = SmallVec<[Event; 1]>;
    type Decoder = Decoder;
    type Acker = TcpNullAcker;

    fn decoder(&self) -> Self::Decoder {
        Decoder::new(
            Framer::OctetCounting(OctetCountingDecoder::new_with_max_length(self.max_length)),
            Deserializer::Syslog(SyslogDeserializerConfig::from_source(SyslogConfig::NAME).build()),
        )
    }

    fn handle_events(&self, events: &mut [Event], host: SocketAddr) {
        handle_events(
            events,
            &self.host_key,
            Some(host.ip().to_string().into()),
            self.log_namespace,
        );
    }

    fn build_acker(&self, _: &[Self::Item]) -> Self::Acker {
        TcpNullAcker
    }
}

pub fn udp(
    addr: SocketListenAddr,
    _max_length: usize,
    host_key: Option<OwnedValuePath>,
    receive_buffer_bytes: Option<usize>,
    shutdown: ShutdownSignal,
    log_namespace: LogNamespace,
    mut out: SourceSender,
) -> super::Source {
    Box::pin(async move {
        let listenfd = ListenFd::from_env();
        let socket = try_bind_udp_socket(addr, listenfd).await.map_err(|error| {
            emit!(SocketBindError {
                mode: SocketMode::Udp,
                error: &error,
            })
        })?;

        if let Some(receive_buffer_bytes) = receive_buffer_bytes
            && let Err(error) = net::set_receive_buffer_size(&socket, receive_buffer_bytes)
        {
            warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
        }

        info!(
            message = "Listening.",
            addr = %addr,
            r#type = "udp"
        );

        let bytes_received = register!(BytesReceived::from(Protocol::UDP));

        let mut stream = UdpFramed::new(
            socket,
            Decoder::new(
                Framer::Bytes(BytesDecoder::new()),
                Deserializer::Syslog(
                    SyslogDeserializerConfig::from_source(SyslogConfig::NAME).build(),
                ),
            ),
        )
        .take_until(shutdown)
        .filter_map(|frame| {
            let host_key = host_key.clone();
            let bytes_received = bytes_received.clone();
            async move {
                match frame {
                    Ok(((mut events, byte_size), received_from)) => {
                        let count = events.len();
                        bytes_received.emit(ByteSize(byte_size));
                        emit!(SocketEventsReceived {
                            mode: SocketMode::Udp,
                            byte_size: events.estimated_json_encoded_size_of(),
                            count,
                        });
                        let received_from = received_from.ip().to_string().into();
                        handle_events(&mut events, &host_key, Some(received_from), log_namespace);
                        Some(events.remove(0))
                    }
                    Err(error) => {
                        emit!(SocketReceiveError {
                            mode: SocketMode::Udp,
                            error: &error,
                        });
                        None
                    }
                }
            }
        })
        .boxed();

        match out.send_event_stream(&mut stream).await {
            Ok(()) => {
                debug!("Finished sending.");
                Ok(())
            }
            Err(_) => {
                let (count, _) = stream.size_hint();
                emit!(StreamClosedError { count });
                Err(())
            }
        }
    })
}

fn handle_events(
    events: &mut [Event],
    host_key: &Option<OwnedValuePath>,
    default_host: Option<Bytes>,
    log_namespace: LogNamespace,
) {
    for event in events {
        enrich_syslog_event(event, host_key, default_host.clone(), log_namespace);
    }
}

fn enrich_syslog_event(
    event: &mut Event,
    host_key: &Option<OwnedValuePath>,
    default_host: Option<Bytes>,
    log_namespace: LogNamespace,
) {
    let log = event.as_mut_log();

    if let Some(default_host) = &default_host {
        log_namespace.insert_source_metadata(
            SyslogConfig::NAME,
            log,
            Some(LegacyKey::Overwrite(path!("source_ip"))),
            path!("source_ip"),
            default_host.clone(),
        );
    }

    let parsed_hostname = log
        .get(event_path!("hostname"))
        .map(|hostname| hostname.coerce_to_bytes());

    if let Some(parsed_host) = parsed_hostname.or(default_host) {
        let legacy_host_key = host_key.as_ref().map(LegacyKey::Overwrite);

        log_namespace.insert_source_metadata(
            SyslogConfig::NAME,
            log,
            legacy_host_key,
            path!("host"),
            parsed_host,
        );
    }

    log_namespace.insert_standard_vector_source_metadata(log, SyslogConfig::NAME, Utc::now());

    if log_namespace == LogNamespace::Legacy {
        let timestamp = log
            .get(event_path!("timestamp"))
            .and_then(|timestamp| timestamp.as_timestamp().cloned())
            .unwrap_or_else(Utc::now);
        log.maybe_insert(log_schema().timestamp_key_target_path(), timestamp);
    }

    trace!(
        message = "Processing one event.",
        event = ?event
    );
}

#[cfg(test)]
mod test;
