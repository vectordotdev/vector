#[cfg(unix)]
use std::path::PathBuf;
use std::{net::SocketAddr, time::Duration};

use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use listenfd::ListenFd;
use smallvec::SmallVec;
use tokio_util::udp::UdpFramed;
use vector_lib::codecs::{
    decoding::{Deserializer, Framer},
    BytesDecoder, OctetCountingDecoder, SyslogDeserializerConfig,
};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, path, OwnedValuePath};
use vrl::event_path;

#[cfg(unix)]
use crate::sources::util::build_unix_stream_source;
use crate::{
    codecs::Decoder,
    config::{
        log_schema, DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceOutput,
    },
    event::Event,
    internal_events::StreamClosedError,
    internal_events::{SocketBindError, SocketMode, SocketReceiveError},
    net,
    shutdown::ShutdownSignal,
    sources::util::net::{try_bind_udp_socket, SocketListenAddr, TcpNullAcker, TcpSource},
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsSourceConfig},
    SourceSender,
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
pub enum Mode {
    /// Listen on TCP.
    Tcp {
        #[configurable(derived)]
        address: SocketListenAddr,

        #[configurable(derived)]
        keepalive: Option<TcpKeepaliveConfig>,

        #[configurable(derived)]
        tls: Option<TlsSourceConfig>,

        /// The size of the receive buffer used for each connection.
        ///
        /// This should not typically needed to be changed.
        #[configurable(metadata(docs::type_unit = "bytes"))]
        receive_buffer_bytes: Option<usize>,

        /// The maximum number of TCP connections that are allowed at any given time.
        connection_limit: Option<u32>,
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
                tls: None,
                receive_buffer_bytes: None,
                connection_limit: None,
            },
            host_key: None,
            max_length: crate::serde::default_max_length(),
            log_namespace: None,
        }
    }
}

impl GenerateConfig for SyslogConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(SyslogConfig::default()).unwrap()
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
                tls,
                receive_buffer_bytes,
                connection_limit,
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
                let tls = MaybeTlsSettings::from_config(&tls_config, true)?;
                source.run(
                    address,
                    keepalive,
                    shutdown_secs,
                    tls,
                    tls_client_metadata_key,
                    receive_buffer_bytes,
                    None,
                    cx,
                    false.into(),
                    connection_limit,
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

        vec![SourceOutput::new_logs(DataType::Log, schema_definition)]
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

        if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            if let Err(error) = net::set_receive_buffer_size(&socket, receive_buffer_bytes) {
                warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
            }
        }

        info!(
            message = "Listening.",
            addr = %addr,
            r#type = "udp"
        );

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
            async move {
                match frame {
                    Ok(((mut events, _byte_size), received_from)) => {
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
mod test {
    use std::{collections::HashMap, fmt, str::FromStr};
    use vector_lib::lookup::{event_path, owned_value_path, OwnedTargetPath};

    use chrono::prelude::*;
    use rand::{thread_rng, Rng};
    use serde::Deserialize;
    use tokio::time::{sleep, Duration, Instant};
    use tokio_util::codec::BytesCodec;
    use vector_lib::assert_event_data_eq;
    use vector_lib::codecs::decoding::format::Deserializer;
    use vector_lib::lookup::PathPrefix;
    use vector_lib::{config::ComponentKey, schema::Definition};
    use vrl::value::{kind::Collection, Kind, ObjectMap, Value};

    use super::*;
    use crate::{
        config::log_schema,
        event::{Event, LogEvent},
        test_util::{
            components::{assert_source_compliance, SOCKET_PUSH_SOURCE_TAGS},
            next_addr, random_maps, random_string, send_encodable, send_lines, wait_for_tcp,
            CountReceiver,
        },
    };

    fn event_from_bytes(
        host_key: &str,
        default_host: Option<Bytes>,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> Option<Event> {
        let parser = SyslogDeserializerConfig::from_source(SyslogConfig::NAME).build();
        let mut events = parser.parse(bytes, LogNamespace::Legacy).ok()?;
        handle_events(
            &mut events,
            &Some(owned_value_path!(host_key)),
            default_host,
            log_namespace,
        );
        Some(events.remove(0))
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SyslogConfig>();
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = SyslogConfig {
            log_namespace: Some(true),
            ..Default::default()
        };

        let definitions = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let expected_definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), "message")
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "timestamp"),
                    Kind::timestamp(),
                    Some("timestamp"),
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "hostname"),
                    Kind::bytes().or_undefined(),
                    Some("host"),
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "source_ip"),
                    Kind::bytes().or_undefined(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "severity"),
                    Kind::bytes().or_undefined(),
                    Some("severity"),
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "facility"),
                    Kind::bytes().or_undefined(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "version"),
                    Kind::integer().or_undefined(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "appname"),
                    Kind::bytes().or_undefined(),
                    Some("service"),
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "msgid"),
                    Kind::bytes().or_undefined(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "procid"),
                    Kind::integer().or_bytes().or_undefined(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "structured_data"),
                    Kind::object(Collection::from_unknown(Kind::object(
                        Collection::from_unknown(Kind::bytes()),
                    ))),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("syslog", "tls_client_metadata"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None,
                );

        assert_eq!(definitions, Some(expected_definition));
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = SyslogConfig::default();

        let definitions = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(
            &owned_value_path!("message"),
            Kind::bytes(),
            Some("message"),
        )
        .with_event_field(
            &owned_value_path!("timestamp"),
            Kind::timestamp(),
            Some("timestamp"),
        )
        .with_event_field(
            &owned_value_path!("hostname"),
            Kind::bytes().or_undefined(),
            Some("host"),
        )
        .with_event_field(
            &owned_value_path!("source_ip"),
            Kind::bytes().or_undefined(),
            None,
        )
        .with_event_field(
            &owned_value_path!("severity"),
            Kind::bytes().or_undefined(),
            Some("severity"),
        )
        .with_event_field(
            &owned_value_path!("facility"),
            Kind::bytes().or_undefined(),
            None,
        )
        .with_event_field(
            &owned_value_path!("version"),
            Kind::integer().or_undefined(),
            None,
        )
        .with_event_field(
            &owned_value_path!("appname"),
            Kind::bytes().or_undefined(),
            Some("service"),
        )
        .with_event_field(
            &owned_value_path!("msgid"),
            Kind::bytes().or_undefined(),
            None,
        )
        .with_event_field(
            &owned_value_path!("procid"),
            Kind::integer().or_bytes().or_undefined(),
            None,
        )
        .unknown_fields(Kind::object(Collection::from_unknown(Kind::bytes())))
        .with_standard_vector_source_metadata();

        assert_eq!(definitions, Some(expected_definition));
    }

    #[test]
    fn config_tcp() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(matches!(config.mode, Mode::Tcp { .. }));
    }

    #[test]
    fn config_tcp_with_receive_buffer_size() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
            receive_buffer_bytes = 256
          "#,
        )
        .unwrap();

        let receive_buffer_bytes = match config.mode {
            Mode::Tcp {
                receive_buffer_bytes,
                ..
            } => receive_buffer_bytes,
            _ => panic!("expected Mode::Tcp"),
        };

        assert_eq!(receive_buffer_bytes, Some(256));
    }

    #[test]
    fn config_tcp_keepalive_empty() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
          "#,
        )
        .unwrap();

        let keepalive = match config.mode {
            Mode::Tcp { keepalive, .. } => keepalive,
            _ => panic!("expected Mode::Tcp"),
        };

        assert_eq!(keepalive, None);
    }

    #[test]
    fn config_tcp_keepalive_full() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
            keepalive.time_secs = 7200
          "#,
        )
        .unwrap();

        let keepalive = match config.mode {
            Mode::Tcp { keepalive, .. } => keepalive,
            _ => panic!("expected Mode::Tcp"),
        };

        let keepalive = keepalive.expect("keepalive config not set");

        assert_eq!(keepalive.time_secs, Some(7200));
    }

    #[test]
    fn config_udp() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "udp"
            address = "127.0.0.1:1235"
            max_length = 32187
          "#,
        )
        .unwrap();
        assert!(matches!(config.mode, Mode::Udp { .. }));
    }

    #[test]
    fn config_udp_with_receive_buffer_size() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "udp"
            address = "127.0.0.1:1235"
            max_length = 32187
            receive_buffer_bytes = 256
          "#,
        )
        .unwrap();

        let receive_buffer_bytes = match config.mode {
            Mode::Udp {
                receive_buffer_bytes,
                ..
            } => receive_buffer_bytes,
            _ => panic!("expected Mode::Udp"),
        };

        assert_eq!(receive_buffer_bytes, Some(256));
    }

    #[cfg(unix)]
    #[test]
    fn config_unix() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "unix"
            path = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(matches!(config.mode, Mode::Unix { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn config_unix_permissions() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "unix"
            path = "127.0.0.1:1235"
            socket_file_mode = 0o777
          "#,
        )
        .unwrap();
        let socket_file_mode = match config.mode {
            Mode::Unix {
                path: _,
                socket_file_mode,
            } => socket_file_mode,
            _ => panic!("expected Mode::Unix"),
        };

        assert_eq!(socket_file_mode, Some(0o777));
    }

    #[test]
    fn syslog_ng_network_syslog_protocol() {
        // this should also match rsyslog omfwd with template=RSYSLOG_SyslogProtocol23Format
        let msg = "i am foobar";
        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {}{} {}"#,
            r#"[meta sequenceId="1" sysUpTime="37" language="EN"]"#,
            r#"[origin ip="192.168.0.1" software="test"]"#,
            msg
        );

        let mut expected = Event::Log(LogEvent::from(msg));

        {
            let expected = expected.as_mut_log();
            expected.insert(
                (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
                Utc.with_ymd_and_hms(2019, 2, 13, 19, 48, 34)
                    .single()
                    .expect("invalid timestamp"),
            );
            expected.insert(
                log_schema().source_type_key_target_path().unwrap(),
                "syslog",
            );
            expected.insert("host", "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");

            expected.insert("meta.sequenceId", "1");
            expected.insert("meta.sysUpTime", "37");
            expected.insert("meta.language", "EN");
            expected.insert("origin.software", "test");
            expected.insert("origin.ip", "192.168.0.1");

            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("version", 1);
            expected.insert("appname", "root");
            expected.insert("procid", 8449);
            expected.insert("source_ip", "192.168.0.254");
        }

        assert_event_data_eq!(
            event_from_bytes(
                "host",
                Some(Bytes::from("192.168.0.254")),
                raw.into(),
                LogNamespace::Legacy
            )
            .unwrap(),
            expected
        );
    }

    #[test]
    fn handles_incorrect_sd_element() {
        let msg = "qwerty";
        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} {}"#,
            r#"[incorrect x]"#, msg
        );

        let mut expected = Event::Log(LogEvent::from(msg));
        {
            let expected = expected.as_mut_log();
            expected.insert(
                (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
                Utc.with_ymd_and_hms(2019, 2, 13, 19, 48, 34)
                    .single()
                    .expect("invalid timestamp"),
            );
            expected.insert(
                log_schema().host_key().unwrap().to_string().as_str(),
                "74794bfb6795",
            );
            expected.insert("hostname", "74794bfb6795");
            expected.insert(
                log_schema().source_type_key_target_path().unwrap(),
                "syslog",
            );
            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("version", 1);
            expected.insert("appname", "root");
            expected.insert("procid", 8449);
            expected.insert("source_ip", "192.168.0.254");
        }

        let event = event_from_bytes(
            "host",
            Some(Bytes::from("192.168.0.254")),
            raw.into(),
            LogNamespace::Legacy,
        )
        .unwrap();
        assert_event_data_eq!(event, expected);

        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} {}"#,
            r#"[incorrect x=]"#, msg
        );

        let event = event_from_bytes(
            "host",
            Some(Bytes::from("192.168.0.254")),
            raw.into(),
            LogNamespace::Legacy,
        )
        .unwrap();
        assert_event_data_eq!(event, expected);
    }

    #[test]
    fn handles_empty_sd_element() {
        fn there_is_map_called_empty(event: Event) -> bool {
            event
                .as_log()
                .get("empty")
                .expect("empty exists")
                .is_object()
        }

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty]"#
        );

        let event = event_from_bytes("host", None, msg.into(), LogNamespace::Legacy).unwrap();
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[non_empty x="1"][empty]"#
        );

        let event = event_from_bytes("host", None, msg.into(), LogNamespace::Legacy).unwrap();
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty][non_empty x="1"]"#
        );

        let event = event_from_bytes("host", None, msg.into(), LogNamespace::Legacy).unwrap();
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty not_really="testing the test"]"#
        );

        let event = event_from_bytes("host", None, msg.into(), LogNamespace::Legacy).unwrap();
        assert!(there_is_map_called_empty(event));
    }

    #[test]
    fn handles_weird_whitespace() {
        // this should also match rsyslog omfwd with template=RSYSLOG_SyslogProtocol23Format
        let raw = r#"
            <13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar
            "#;
        let cleaned = r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar"#;

        assert_event_data_eq!(
            event_from_bytes("host", None, raw.to_owned().into(), LogNamespace::Legacy).unwrap(),
            event_from_bytes(
                "host",
                None,
                cleaned.to_owned().into(),
                LogNamespace::Legacy
            )
            .unwrap()
        );
    }

    #[test]
    fn handles_dots_in_sdata() {
        let raw =
            r#"<190>Feb 13 21:31:56 74794bfb6795 liblogging-stdlog:  [origin foo.bar="baz"] hello"#;
        let event =
            event_from_bytes("host", None, raw.to_owned().into(), LogNamespace::Legacy).unwrap();
        assert_eq!(
            event.as_log().get(r#"origin."foo.bar""#),
            Some(&Value::from("baz"))
        );
    }

    #[test]
    fn syslog_ng_default_network() {
        let msg = "i am foobar";
        let raw = format!(r#"<13>Feb 13 20:07:26 74794bfb6795 root[8539]: {}"#, msg);
        let event = event_from_bytes(
            "host",
            Some(Bytes::from("192.168.0.254")),
            raw.into(),
            LogNamespace::Legacy,
        )
        .unwrap();

        let mut expected = Event::Log(LogEvent::from(msg));
        {
            let value = event.as_log().get("timestamp").unwrap();
            let year = value.as_timestamp().unwrap().naive_local().year();

            let expected = expected.as_mut_log();
            let expected_date: DateTime<Utc> = Local
                .with_ymd_and_hms(year, 2, 13, 20, 7, 26)
                .single()
                .expect("invalid timestamp")
                .into();

            expected.insert(
                (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
                expected_date,
            );
            expected.insert(
                log_schema().host_key().unwrap().to_string().as_str(),
                "74794bfb6795",
            );
            expected.insert(
                log_schema().source_type_key_target_path().unwrap(),
                "syslog",
            );
            expected.insert("hostname", "74794bfb6795");
            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("appname", "root");
            expected.insert("procid", 8539);
            expected.insert("source_ip", "192.168.0.254");
        }

        assert_event_data_eq!(event, expected);
    }

    #[test]
    fn rsyslog_omfwd_tcp_default() {
        let msg = "start";
        let raw = format!(
            r#"<190>Feb 13 21:31:56 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="8979" x-info="http://www.rsyslog.com"] {}"#,
            msg
        );
        let event = event_from_bytes(
            "host",
            Some(Bytes::from("192.168.0.254")),
            raw.into(),
            LogNamespace::Legacy,
        )
        .unwrap();

        let mut expected = Event::Log(LogEvent::from(msg));
        {
            let value = event.as_log().get("timestamp").unwrap();
            let year = value.as_timestamp().unwrap().naive_local().year();

            let expected = expected.as_mut_log();
            let expected_date: DateTime<Utc> = Local
                .with_ymd_and_hms(year, 2, 13, 21, 31, 56)
                .single()
                .expect("invalid timestamp")
                .into();
            expected.insert(
                (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
                expected_date,
            );
            expected.insert(
                log_schema().source_type_key_target_path().unwrap(),
                "syslog",
            );
            expected.insert("host", "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");
            expected.insert("severity", "info");
            expected.insert("facility", "local7");
            expected.insert("appname", "liblogging-stdlog");
            expected.insert("origin.software", "rsyslogd");
            expected.insert("origin.swVersion", "8.24.0");
            expected.insert("source_ip", "192.168.0.254");
            expected.insert(event_path!("origin", "x-pid"), "8979");
            expected.insert(event_path!("origin", "x-info"), "http://www.rsyslog.com");
        }

        assert_event_data_eq!(event, expected);
    }

    #[test]
    fn rsyslog_omfwd_tcp_forward_format() {
        let msg = "start";
        let raw = format!(
            r#"<190>2019-02-13T21:53:30.605850+00:00 74794bfb6795 liblogging-stdlog:  [origin software="rsyslogd" swVersion="8.24.0" x-pid="9043" x-info="http://www.rsyslog.com"] {}"#,
            msg
        );

        let mut expected = Event::Log(LogEvent::from(msg));
        {
            let expected = expected.as_mut_log();
            expected.insert(
                (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
                Utc.with_ymd_and_hms(2019, 2, 13, 21, 53, 30)
                    .single()
                    .and_then(|t| t.with_nanosecond(605_850 * 1000))
                    .expect("invalid timestamp"),
            );
            expected.insert(
                log_schema().source_type_key_target_path().unwrap(),
                "syslog",
            );
            expected.insert("host", "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");
            expected.insert("severity", "info");
            expected.insert("facility", "local7");
            expected.insert("appname", "liblogging-stdlog");
            expected.insert("origin.software", "rsyslogd");
            expected.insert("origin.swVersion", "8.24.0");
            expected.insert(event_path!("origin", "x-pid"), "9043");
            expected.insert(event_path!("origin", "x-info"), "http://www.rsyslog.com");
        }

        assert_event_data_eq!(
            event_from_bytes("host", None, raw.into(), LogNamespace::Legacy).unwrap(),
            expected
        );
    }

    #[tokio::test]
    async fn test_tcp_syslog() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let num_messages: usize = 10000;
            let in_addr = next_addr();

            // Create and spawn the source.
            let config = SyslogConfig::from_mode(Mode::Tcp {
                address: in_addr.into(),
                keepalive: None,
                tls: None,
                receive_buffer_bytes: None,
                connection_limit: None,
            });

            let key = ComponentKey::from("in");
            let (tx, rx) = SourceSender::new_test();
            let (context, shutdown) = SourceContext::new_shutdown(&key, tx);
            let shutdown_complete = shutdown.shutdown_tripwire();

            let source = config
                .build(context)
                .await
                .expect("source should not fail to build");
            tokio::spawn(source);

            // Wait for source to become ready to accept traffic.
            wait_for_tcp(in_addr).await;

            let output_events = CountReceiver::receive_events(rx);

            // Now craft and send syslog messages to the source, and collect them on the other side.
            let input_messages: Vec<SyslogMessageRfc5424> = (0..num_messages)
                .map(|i| SyslogMessageRfc5424::random(i, 30, 4, 3, 3))
                .collect();

            let input_lines: Vec<String> =
                input_messages.iter().map(|msg| msg.to_string()).collect();

            send_lines(in_addr, input_lines).await.unwrap();

            // Wait a short period of time to ensure the messages get sent.
            sleep(Duration::from_secs(2)).await;

            // Shutdown the source, and make sure we've got all the messages we sent in.
            shutdown
                .shutdown_all(Some(Instant::now() + Duration::from_millis(100)))
                .await;
            shutdown_complete.await;

            let output_events = output_events.await;
            assert_eq!(output_events.len(), num_messages);

            let output_messages: Vec<SyslogMessageRfc5424> = output_events
                .into_iter()
                .map(|mut e| {
                    e.as_mut_log().remove("hostname"); // Vector adds this field which will cause a parse error.
                    e.as_mut_log().remove("source_ip"); // Vector adds this field which will cause a parse error.
                    e.into()
                })
                .collect();
            assert_eq!(output_messages, input_messages);
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_unix_stream_syslog() {
        use crate::test_util::components::SOCKET_PUSH_SOURCE_TAGS;
        use futures_util::{stream, SinkExt};
        use std::os::unix::net::UnixStream as StdUnixStream;
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixStream;
        use tokio_util::codec::{FramedWrite, LinesCodec};

        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let num_messages: usize = 1;
            let in_path = tempfile::tempdir().unwrap().into_path().join("stream_test");

            // Create and spawn the source.
            let config = SyslogConfig::from_mode(Mode::Unix {
                path: in_path.clone(),
                socket_file_mode: None,
            });

            let key = ComponentKey::from("in");
            let (tx, rx) = SourceSender::new_test();
            let (context, shutdown) = SourceContext::new_shutdown(&key, tx);
            let shutdown_complete = shutdown.shutdown_tripwire();

            let source = config
                .build(context)
                .await
                .expect("source should not fail to build");
            tokio::spawn(source);

            // Wait for source to become ready to accept traffic.
            while StdUnixStream::connect(&in_path).is_err() {
                tokio::task::yield_now().await;
            }

            let output_events = CountReceiver::receive_events(rx);

            // Now craft and send syslog messages to the source, and collect them on the other side.
            let input_messages: Vec<SyslogMessageRfc5424> = (0..num_messages)
                .map(|i| SyslogMessageRfc5424::random(i, 30, 4, 3, 3))
                .collect();

            let stream = UnixStream::connect(&in_path).await.unwrap();
            let mut sink = FramedWrite::new(stream, LinesCodec::new());

            let lines: Vec<String> = input_messages.iter().map(|msg| msg.to_string()).collect();
            let mut lines = stream::iter(lines).map(Ok);
            sink.send_all(&mut lines).await.unwrap();

            let stream = sink.get_mut();
            stream.shutdown().await.unwrap();

            // Wait a short period of time to ensure the messages get sent.
            sleep(Duration::from_secs(1)).await;

            shutdown
                .shutdown_all(Some(Instant::now() + Duration::from_millis(100)))
                .await;
            shutdown_complete.await;

            let output_events = output_events.await;
            assert_eq!(output_events.len(), num_messages);

            let output_messages: Vec<SyslogMessageRfc5424> = output_events
                .into_iter()
                .map(|mut e| {
                    e.as_mut_log().remove("hostname"); // Vector adds this field which will cause a parse error.
                    e.as_mut_log().remove("source_ip"); // Vector adds this field which will cause a parse error.
                    e.into()
                })
                .collect();
            assert_eq!(output_messages, input_messages);
        })
        .await;
    }

    #[tokio::test]
    async fn test_octet_counting_syslog() {
        assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let num_messages: usize = 10000;
            let in_addr = next_addr();

            // Create and spawn the source.
            let config = SyslogConfig::from_mode(Mode::Tcp {
                address: in_addr.into(),
                keepalive: None,
                tls: None,
                receive_buffer_bytes: None,
                connection_limit: None,
            });

            let key = ComponentKey::from("in");
            let (tx, rx) = SourceSender::new_test();
            let (context, shutdown) = SourceContext::new_shutdown(&key, tx);
            let shutdown_complete = shutdown.shutdown_tripwire();

            let source = config
                .build(context)
                .await
                .expect("source should not fail to build");
            tokio::spawn(source);

            // Wait for source to become ready to accept traffic.
            wait_for_tcp(in_addr).await;

            let output_events = CountReceiver::receive_events(rx);

            // Now craft and send syslog messages to the source, and collect them on the other side.
            let input_messages: Vec<SyslogMessageRfc5424> = (0..num_messages)
                .map(|i| {
                    let mut msg = SyslogMessageRfc5424::random(i, 30, 4, 3, 3);
                    msg.message.push('\n');
                    msg.message.push_str(&random_string(30));
                    msg
                })
                .collect();

            let codec = BytesCodec::new();
            let input_lines: Vec<Bytes> = input_messages
                .iter()
                .map(|msg| {
                    let s = msg.to_string();
                    format!("{} {}", s.len(), s).into()
                })
                .collect();

            send_encodable(in_addr, codec, input_lines).await.unwrap();

            // Wait a short period of time to ensure the messages get sent.
            sleep(Duration::from_secs(2)).await;

            // Shutdown the source, and make sure we've got all the messages we sent in.
            shutdown
                .shutdown_all(Some(Instant::now() + Duration::from_millis(100)))
                .await;
            shutdown_complete.await;

            let output_events = output_events.await;
            assert_eq!(output_events.len(), num_messages);

            let output_messages: Vec<SyslogMessageRfc5424> = output_events
                .into_iter()
                .map(|mut e| {
                    e.as_mut_log().remove("hostname"); // Vector adds this field which will cause a parse error.
                    e.as_mut_log().remove("source_ip"); // Vector adds this field which will cause a parse error.
                    e.into()
                })
                .collect();
            assert_eq!(output_messages, input_messages);
        })
        .await;
    }

    #[derive(Deserialize, PartialEq, Clone, Debug)]
    struct SyslogMessageRfc5424 {
        msgid: String,
        severity: Severity,
        facility: Facility,
        version: u8,
        timestamp: String,
        host: String,
        source_type: String,
        appname: String,
        procid: usize,
        message: String,
        #[serde(flatten)]
        structured_data: StructuredData,
    }

    impl SyslogMessageRfc5424 {
        fn random(
            id: usize,
            msg_len: usize,
            field_len: usize,
            max_map_size: usize,
            max_children: usize,
        ) -> Self {
            let msg = random_string(msg_len);
            let structured_data = random_structured_data(max_map_size, max_children, field_len);

            let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
            //"secfrac" can contain up to 6 digits, but TCP sinks uses `AutoSi`

            Self {
                msgid: format!("test{}", id),
                severity: Severity::LOG_INFO,
                facility: Facility::LOG_USER,
                version: 1,
                timestamp,
                host: "hogwarts".to_owned(),
                source_type: "syslog".to_owned(),
                appname: "harry".to_owned(),
                procid: thread_rng().gen_range(0..32768),
                structured_data,
                message: msg,
            }
        }
    }

    impl fmt::Display for SyslogMessageRfc5424 {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(
                f,
                "<{}>{} {} {} {} {} {} {} {}",
                encode_priority(self.severity, self.facility),
                self.version,
                self.timestamp,
                self.host,
                self.appname,
                self.procid,
                self.msgid,
                format_structured_data_rfc5424(&self.structured_data),
                self.message
            )
        }
    }

    impl From<Event> for SyslogMessageRfc5424 {
        fn from(e: Event) -> Self {
            let (value, _) = e.into_log().into_parts();
            let mut fields = value.into_object().unwrap();

            Self {
                msgid: fields.remove("msgid").map(value_to_string).unwrap(),
                severity: fields
                    .remove("severity")
                    .map(value_to_string)
                    .and_then(|s| Severity::from_str(s.as_str()))
                    .unwrap(),
                facility: fields
                    .remove("facility")
                    .map(value_to_string)
                    .and_then(|s| Facility::from_str(s.as_str()))
                    .unwrap(),
                version: fields
                    .remove("version")
                    .map(value_to_string)
                    .map(|s| u8::from_str(s.as_str()).unwrap())
                    .unwrap(),
                timestamp: fields.remove("timestamp").map(value_to_string).unwrap(),
                host: fields.remove("host").map(value_to_string).unwrap(),
                source_type: fields.remove("source_type").map(value_to_string).unwrap(),
                appname: fields.remove("appname").map(value_to_string).unwrap(),
                procid: fields
                    .remove("procid")
                    .map(value_to_string)
                    .map(|s| usize::from_str(s.as_str()).unwrap())
                    .unwrap(),
                message: fields.remove("message").map(value_to_string).unwrap(),
                structured_data: structured_data_from_fields(fields),
            }
        }
    }

    fn structured_data_from_fields(fields: ObjectMap) -> StructuredData {
        let mut structured_data = StructuredData::default();

        for (key, value) in fields.into_iter() {
            let subfields = value
                .into_object()
                .unwrap()
                .into_iter()
                .map(|(k, v)| (k.into(), value_to_string(v)))
                .collect();

            structured_data.insert(key.into(), subfields);
        }

        structured_data
    }

    #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
    #[derive(Copy, Clone, Deserialize, PartialEq, Eq, Debug)]
    pub enum Severity {
        #[serde(rename(deserialize = "emergency"))]
        LOG_EMERG,
        #[serde(rename(deserialize = "alert"))]
        LOG_ALERT,
        #[serde(rename(deserialize = "critical"))]
        LOG_CRIT,
        #[serde(rename(deserialize = "error"))]
        LOG_ERR,
        #[serde(rename(deserialize = "warn"))]
        LOG_WARNING,
        #[serde(rename(deserialize = "notice"))]
        LOG_NOTICE,
        #[serde(rename(deserialize = "info"))]
        LOG_INFO,
        #[serde(rename(deserialize = "debug"))]
        LOG_DEBUG,
    }

    impl Severity {
        fn from_str(s: &str) -> Option<Self> {
            match s {
                "emergency" => Some(Self::LOG_EMERG),
                "alert" => Some(Self::LOG_ALERT),
                "critical" => Some(Self::LOG_CRIT),
                "error" => Some(Self::LOG_ERR),
                "warn" => Some(Self::LOG_WARNING),
                "notice" => Some(Self::LOG_NOTICE),
                "info" => Some(Self::LOG_INFO),
                "debug" => Some(Self::LOG_DEBUG),

                x => {
                    #[allow(clippy::print_stdout)]
                    {
                        println!("converting severity str, got {}", x);
                    }
                    None
                }
            }
        }
    }

    #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
    #[derive(Copy, Clone, PartialEq, Eq, Deserialize, Debug)]
    pub enum Facility {
        #[serde(rename(deserialize = "kernel"))]
        LOG_KERN = 0 << 3,
        #[serde(rename(deserialize = "user"))]
        LOG_USER = 1 << 3,
        #[serde(rename(deserialize = "mail"))]
        LOG_MAIL = 2 << 3,
        #[serde(rename(deserialize = "daemon"))]
        LOG_DAEMON = 3 << 3,
        #[serde(rename(deserialize = "auth"))]
        LOG_AUTH = 4 << 3,
        #[serde(rename(deserialize = "syslog"))]
        LOG_SYSLOG = 5 << 3,
    }

    impl Facility {
        fn from_str(s: &str) -> Option<Self> {
            match s {
                "kernel" => Some(Self::LOG_KERN),
                "user" => Some(Self::LOG_USER),
                "mail" => Some(Self::LOG_MAIL),
                "daemon" => Some(Self::LOG_DAEMON),
                "auth" => Some(Self::LOG_AUTH),
                "syslog" => Some(Self::LOG_SYSLOG),
                _ => None,
            }
        }
    }

    type StructuredData = HashMap<String, HashMap<String, String>>;

    fn random_structured_data(
        max_map_size: usize,
        max_children: usize,
        field_len: usize,
    ) -> StructuredData {
        let amount = thread_rng().gen_range(0..max_children);

        random_maps(max_map_size, field_len)
            .filter(|m| !m.is_empty()) //syslog_rfc5424 ignores empty maps, tested separately
            .take(amount)
            .enumerate()
            .map(|(i, map)| (format!("id{}", i), map))
            .collect()
    }

    fn format_structured_data_rfc5424(data: &StructuredData) -> String {
        if data.is_empty() {
            "-".to_string()
        } else {
            let mut res = String::new();
            for (id, params) in data {
                res = res + "[" + id;
                for (name, value) in params {
                    res = res + " " + name + "=\"" + value + "\"";
                }
                res += "]";
            }

            res
        }
    }

    const fn encode_priority(severity: Severity, facility: Facility) -> u8 {
        facility as u8 | severity as u8
    }

    fn value_to_string(v: Value) -> String {
        if v.is_bytes() {
            let buf = v.as_bytes().unwrap();
            String::from_utf8_lossy(buf).to_string()
        } else if v.is_timestamp() {
            let ts = v.as_timestamp().unwrap();
            ts.to_rfc3339_opts(SecondsFormat::AutoSi, true)
        } else {
            v.to_string()
        }
    }
}
