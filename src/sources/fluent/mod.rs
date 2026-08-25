use std::{collections::HashMap, io, net::SocketAddr, num::NonZeroU64, time::Duration};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::{Buf, Bytes, BytesMut};
use chrono::Utc;
use rmp_serde::{Deserializer, Serializer, decode};
use serde::{Deserialize, Serialize};
use smallvec::{SmallVec, smallvec};
use tokio_util::codec::Decoder;
use vector_lib::{
    codecs::{BytesDeserializerConfig, StreamDecodingError},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    ipallowlist::IpAllowlistConfig,
    lookup::{OwnedValuePath, lookup_v2::parse_value_path, metadata_path, owned_value_path, path},
    schema::Definition,
};
use vrl::value::{Kind, Value, kind::Collection};

use super::util::decompression::{CappedDecoder, max_decompressed_size_bytes};
use super::util::net::{SocketListenAddr, TcpSource, TcpSourceAck, TcpSourceAcker};
use crate::{
    config::{
        DataType, GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig,
        SourceContext, SourceOutput, log_schema,
    },
    event::{Event, LogEvent},
    internal_events::{FluentMessageDecodeError, FluentMessageReceived},
    serde::bool_or_struct,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsSourceConfig},
};

mod message;
use self::message::{FluentEntry, FluentMessage, FluentRecord, FluentTag, FluentTimestamp};

/// Configuration for the `fluent` source.
#[configurable_component(source("fluent", "Collect logs from a Fluentd or Fluent Bit agent."))]
#[derive(Clone, Debug)]
pub struct FluentConfig {
    #[serde(flatten)]
    mode: FluentMode,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,
}

/// Listening mode for the `fluent` source.
#[configurable_component(no_deser)]
#[derive(Clone, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of socket to use."))]
#[allow(clippy::large_enum_variant)] // just used for configuration
pub enum FluentMode {
    /// Listen on TCP port
    Tcp(FluentTcpConfig),

    /// Listen on unix stream socket
    #[cfg(unix)]
    Unix(FluentUnixConfig),
}

/// Serde doesn't provide a way to specify a default tagged variant when deserializing
/// So we use a somewhat arcane setup with an untagged and tagged versions to allow
/// users to not have to specify mode = tcp
///
/// See [serde-rs/serde#2231](https://github.com/serde-rs/serde/issues/2231)
mod deser {
    use super::*;

    #[allow(clippy::large_enum_variant)]
    #[derive(Deserialize)]
    #[serde(tag = "mode")]
    enum FluentModeTagged {
        #[serde(rename = "tcp")]
        Tcp(FluentTcpConfig),

        #[cfg(unix)]
        #[serde(rename = "unix")]
        Unix(FluentUnixConfig),
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum FluentModeDe {
        Tagged(FluentModeTagged),

        // Note: this must be last as serde attempts variants in order
        Untagged(FluentTcpConfig),
    }

    impl<'de> Deserialize<'de> for FluentMode {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            Ok(match FluentModeDe::deserialize(deserializer)? {
                FluentModeDe::Tagged(FluentModeTagged::Tcp(config)) => FluentMode::Tcp(config),
                #[cfg(unix)]
                FluentModeDe::Tagged(FluentModeTagged::Unix(config)) => FluentMode::Unix(config),
                FluentModeDe::Untagged(config) => FluentMode::Tcp(config),
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_tcp_default_mode() {
            let json_data = serde_json::json!({
                "address": "0.0.0.0:2020",
                "connection_limit": 2
            });

            let parsed: FluentConfig = serde_json::from_value(json_data).unwrap();
            assert!(matches!(parsed.mode, FluentMode::Tcp(c) if c.connection_limit.unwrap() == 2));
        }

        #[test]
        fn test_tcp_explicit_mode() {
            let json_data = serde_json::json!({
                "mode": "tcp",
                "address": "0.0.0.0:2020",
                "connection_limit": 2
            });

            let parsed: FluentConfig = serde_json::from_value(json_data).unwrap();
            assert!(matches!(parsed.mode, FluentMode::Tcp(c) if c.connection_limit.unwrap() == 2));
        }

        #[test]
        fn test_invalid_unix_mode() {
            let json_data = serde_json::json!({
                "mode": "unix",
                "address": "0.0.0.0:2020",
                "connection_limit": 2
            });

            assert!(serde_json::from_value::<FluentConfig>(json_data).is_err());
        }

        #[cfg(unix)]
        #[test]
        fn test_valid_unix_mode() {
            let json_data = serde_json::json!({
                "mode": "unix",
                "path": "/foo"
            });

            let parsed: FluentConfig = serde_json::from_value(json_data).unwrap();
            assert!(
                matches!(parsed.mode, FluentMode::Unix(c) if c.path.to_string_lossy() == "/foo")
            );
        }
    }
}

/// Configuration for the `fluent` TCP source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FluentTcpConfig {
    #[configurable(derived)]
    address: SocketListenAddr,

    /// The maximum number of TCP connections that are allowed at any given time.
    #[configurable(metadata(docs::type_unit = "connections"))]
    connection_limit: Option<u32>,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    #[configurable(derived)]
    pub permit_origin: Option<IpAllowlistConfig>,

    /// The size of the receive buffer used for each connection.
    ///
    /// This generally should not need to be changed.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    receive_buffer_bytes: Option<usize>,

    /// The timeout, in seconds, before a TLS handshake is aborted if it has not completed.
    ///
    /// This bounds how long a connection can hold its slot against `connection_limit`
    /// before the TLS handshake finishes, protecting against clients that open a
    /// connection but never complete (or never start) a handshake.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    tls_handshake_timeout_secs: Option<NonZeroU64>,

    #[configurable(derived)]
    tls: Option<TlsSourceConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,
}

impl FluentTcpConfig {
    fn build(
        &self,
        cx: SourceContext,
        log_namespace: LogNamespace,
    ) -> crate::Result<super::Source> {
        let source = FluentSource::new(log_namespace);
        let shutdown_secs = Duration::from_secs(30);
        let tls_config = self.tls.as_ref().map(|tls| tls.tls_config.clone());
        let tls_client_metadata_key = self
            .tls
            .as_ref()
            .and_then(|tls| tls.client_metadata_key.clone())
            .and_then(|k| k.path);
        let tls = MaybeTlsSettings::from_config(tls_config.as_ref(), true)?;
        source.run(
            self.address,
            self.keepalive,
            shutdown_secs,
            tls,
            None, // tls_reloader: not wired for this source
            tls_client_metadata_key,
            self.receive_buffer_bytes,
            None,
            self.tls_handshake_timeout_secs,
            cx,
            self.acknowledgements,
            self.connection_limit,
            self.permit_origin.clone().map(Into::into),
            FluentConfig::NAME,
            log_namespace,
        )
    }
}

/// Configuration for the `fluent` unix socket source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
#[cfg(unix)]
pub struct FluentUnixConfig {
    /// The Unix socket path.
    ///
    /// This should be an absolute path.
    #[configurable(metadata(docs::examples = "/path/to/socket"))]
    pub path: std::path::PathBuf,

    /// Unix file mode bits to be applied to the unix socket file as its designated file permissions.
    ///
    /// Note: The file mode value can be specified in any numeric format supported by your configuration
    /// language, but it is most intuitive to use an octal number.
    #[configurable(metadata(docs::examples = 0o777))]
    #[configurable(metadata(docs::examples = 0o600))]
    #[configurable(metadata(docs::examples = 508))]
    pub socket_file_mode: Option<u32>,
}

#[cfg(unix)]
impl FluentUnixConfig {
    fn build(
        &self,
        cx: SourceContext,
        log_namespace: LogNamespace,
    ) -> crate::Result<super::Source> {
        let source = FluentSource::new(log_namespace);

        crate::sources::util::build_unix_stream_source(
            self.path.clone(),
            self.socket_file_mode,
            source.decoder(),
            move |events, host| source.handle_events_impl(events, host.into()),
            cx.shutdown,
            cx.out,
        )
    }
}

impl GenerateConfig for FluentConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            mode: FluentMode::Tcp(FluentTcpConfig {
                address: SocketListenAddr::SocketAddr("0.0.0.0:24224".parse().unwrap()),
                keepalive: None,
                permit_origin: None,
                tls: None,
                receive_buffer_bytes: None,
                tls_handshake_timeout_secs: None,
                acknowledgements: Default::default(),
                connection_limit: Some(2),
            }),
            log_namespace: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "fluent")]
impl SourceConfig for FluentConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        match &self.mode {
            FluentMode::Tcp(t) => t.build(cx, log_namespace),
            #[cfg(unix)]
            FluentMode::Unix(u) => u.build(cx, log_namespace),
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = self.schema_definition(log_namespace);

        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        match &self.mode {
            FluentMode::Tcp(tcp) => vec![tcp.address.as_tcp_resource()],
            #[cfg(unix)]
            FluentMode::Unix(_) => vec![],
        }
    }

    fn can_acknowledge(&self) -> bool {
        matches!(self.mode, FluentMode::Tcp(_))
    }
}

impl FluentConfig {
    /// Builds the `schema::Definition` for this source using the provided `LogNamespace`.
    fn schema_definition(&self, log_namespace: LogNamespace) -> Definition {
        // `host_key` is only inserted if not present already.
        let host_key = log_schema()
            .host_key()
            .cloned()
            .map(LegacyKey::InsertIfEmpty);

        let tag_key = parse_value_path("tag").ok().map(LegacyKey::Overwrite);

        let tls_client_metadata_path = match &self.mode {
            FluentMode::Tcp(tcp) => tcp
                .tls
                .as_ref()
                .and_then(|tls| tls.client_metadata_key.as_ref())
                .and_then(|k| k.path.clone())
                .map(LegacyKey::Overwrite),
            #[cfg(unix)]
            FluentMode::Unix(_) => None,
        };

        // There is a global and per-source `log_namespace` config.
        // The source config overrides the global setting and is merged here.
        let mut schema_definition = BytesDeserializerConfig
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                FluentConfig::NAME,
                host_key,
                &owned_value_path!("host"),
                Kind::bytes(),
                Some("host"),
            )
            .with_source_metadata(
                FluentConfig::NAME,
                tag_key,
                &owned_value_path!("tag"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                FluentConfig::NAME,
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            // for metadata that is added to the events dynamically from the FluentRecord
            .with_source_metadata(
                FluentConfig::NAME,
                None,
                &owned_value_path!("record"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                tls_client_metadata_path,
                &owned_value_path!("tls_client_metadata"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            );

        // for metadata that is added to the events dynamically
        if log_namespace == LogNamespace::Legacy {
            schema_definition = schema_definition.unknown_fields(Kind::bytes());
        }

        schema_definition
    }
}

#[derive(Debug, Clone)]
struct FluentSource {
    log_namespace: LogNamespace,
    legacy_host_key_path: Option<OwnedValuePath>,
}

impl FluentSource {
    fn new(log_namespace: LogNamespace) -> Self {
        Self {
            log_namespace,
            legacy_host_key_path: log_schema().host_key().cloned(),
        }
    }

    fn handle_events_impl(&self, events: &mut [Event], host: Value) {
        for event in events {
            let log = event.as_mut_log();

            let legacy_host_key = self
                .legacy_host_key_path
                .as_ref()
                .map(LegacyKey::InsertIfEmpty);

            self.log_namespace.insert_source_metadata(
                FluentConfig::NAME,
                log,
                legacy_host_key,
                path!("host"),
                host.clone(),
            );
        }
    }
}

impl TcpSource for FluentSource {
    type Error = DecodeError;
    type Item = FluentFrame;
    type Decoder = FluentDecoder;
    type Acker = FluentAcker;

    fn decoder(&self) -> Self::Decoder {
        FluentDecoder::new(self.log_namespace)
    }

    fn handle_events(&self, events: &mut [Event], host: SocketAddr) {
        self.handle_events_impl(events, host.ip().to_string().into())
    }

    fn build_acker(&self, frame: &[Self::Item]) -> Self::Acker {
        FluentAcker::new(frame)
    }
}

#[derive(Debug)]
pub enum DecodeError {
    IO(io::Error),
    Decode(decode::Error),
    UnknownCompression(String),
    UnexpectedValue(rmpv::Value),
    /// The buffered frame grew past the maximum allowed size before a complete
    /// message could be decoded. Emitted to bound memory when a peer declares an
    /// oversized msgpack array/map/string and streams the bytes to force
    /// unbounded buffering.
    FrameTooLarge {
        size: usize,
        max: usize,
    },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::IO(err) => write!(f, "{err}"),
            DecodeError::Decode(err) => write!(f, "{err}"),
            DecodeError::UnknownCompression(compression) => {
                write!(f, "unknown compression: {compression}")
            }
            DecodeError::UnexpectedValue(value) => {
                write!(f, "unexpected msgpack value, ignoring: {value}")
            }
            DecodeError::FrameTooLarge { size, max } => {
                write!(
                    f,
                    "fluent frame exceeds maximum size before decoding: {size} bytes buffered, limit is {max} bytes"
                )
            }
        }
    }
}

impl StreamDecodingError for DecodeError {
    fn can_continue(&self) -> bool {
        match self {
            DecodeError::IO(_) => false,
            DecodeError::Decode(_) => true,
            DecodeError::UnknownCompression(_) => true,
            DecodeError::UnexpectedValue(_) => true,
            // An oversized partial frame has no framing boundary to resync on, so
            // the connection must be dropped rather than re-decoded in a loop.
            DecodeError::FrameTooLarge { .. } => false,
        }
    }
}

impl From<io::Error> for DecodeError {
    fn from(e: io::Error) -> Self {
        DecodeError::IO(e)
    }
}

impl From<decode::Error> for DecodeError {
    fn from(e: decode::Error) -> Self {
        DecodeError::Decode(e)
    }
}

#[derive(Debug, Clone)]
struct FluentDecoder {
    log_namespace: LogNamespace,
    /// Maximum number of bytes that may be buffered while waiting for a complete
    /// frame. Bounds memory against a peer that declares an oversized msgpack
    /// structure and streams the bytes to force unbounded buffering.
    max_frame_size: usize,
}

impl FluentDecoder {
    fn new(log_namespace: LogNamespace) -> Self {
        Self {
            log_namespace,
            max_frame_size: max_decompressed_size_bytes(),
        }
    }

    fn handle_message(
        &mut self,
        message: Result<FluentMessage, DecodeError>,
        byte_size: usize,
    ) -> Result<Option<(FluentFrame, usize)>, DecodeError> {
        let log_namespace = &self.log_namespace;

        match message? {
            FluentMessage::Message(tag, timestamp, record) => {
                let event = Event::from(FluentEvent {
                    tag,
                    timestamp,
                    record,
                    log_namespace,
                });
                let frame = FluentFrame {
                    events: smallvec![event],
                    chunk: None,
                };
                Ok(Some((frame, byte_size)))
            }
            FluentMessage::MessageWithOptions(tag, timestamp, record, options) => {
                let event = Event::from(FluentEvent {
                    tag,
                    timestamp,
                    record,
                    log_namespace,
                });
                let frame = FluentFrame {
                    events: smallvec![event],
                    chunk: options.chunk,
                };
                Ok(Some((frame, byte_size)))
            }
            FluentMessage::Forward(tag, entries) => {
                let events = entries
                    .into_iter()
                    .map(|FluentEntry(timestamp, record)| {
                        Event::from(FluentEvent {
                            tag: tag.clone(),
                            timestamp,
                            record,
                            log_namespace,
                        })
                    })
                    .collect();
                let frame = FluentFrame {
                    events,
                    chunk: None,
                };
                Ok(Some((frame, byte_size)))
            }
            FluentMessage::ForwardWithOptions(tag, entries, options) => {
                let events = entries
                    .into_iter()
                    .map(|FluentEntry(timestamp, record)| {
                        Event::from(FluentEvent {
                            tag: tag.clone(),
                            timestamp,
                            record,
                            log_namespace,
                        })
                    })
                    .collect();
                let frame = FluentFrame {
                    events,
                    chunk: options.chunk,
                };
                Ok(Some((frame, byte_size)))
            }
            FluentMessage::PackedForward(tag, bin) => {
                let mut buf = BytesMut::from(&bin[..]);

                let mut events = smallvec![];
                while let Some(FluentEntry(timestamp, record)) =
                    FluentEntryStreamDecoder.decode(&mut buf)?
                {
                    events.push(Event::from(FluentEvent {
                        tag: tag.clone(),
                        timestamp,
                        record,
                        log_namespace,
                    }));
                }
                let frame = FluentFrame {
                    events,
                    chunk: None,
                };
                Ok(Some((frame, byte_size)))
            }
            FluentMessage::PackedForwardWithOptions(tag, bin, options) => {
                let buf = match options.compressed.as_deref() {
                    // Cap the decompressed output so a `gzip` bomb in a single
                    // `PackedForward` message cannot drive unbounded allocation.
                    Some("gzip") => CappedDecoder::gzip(io::Cursor::new(bin.into_vec()))
                        .decompress()
                        .map_err(Into::into),
                    Some("text") | None => Ok(bin.into_vec()),
                    Some(s) => Err(DecodeError::UnknownCompression(s.to_owned())),
                }?;

                let mut buf = BytesMut::from(&buf[..]);

                let mut events = smallvec![];
                while let Some(FluentEntry(timestamp, record)) =
                    FluentEntryStreamDecoder.decode(&mut buf)?
                {
                    events.push(Event::from(FluentEvent {
                        tag: tag.clone(),
                        timestamp,
                        record,
                        log_namespace,
                    }));
                }
                let frame = FluentFrame {
                    events,
                    chunk: options.chunk,
                };
                Ok(Some((frame, byte_size)))
            }
            FluentMessage::Heartbeat(rmpv::Value::Nil) => Ok(None),
            FluentMessage::Heartbeat(value) => Err(DecodeError::UnexpectedValue(value)),
        }
    }
}

impl Decoder for FluentDecoder {
    type Item = (FluentFrame, usize);
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            if src.is_empty() {
                return Ok(None);
            }

            let (byte_size, res) = {
                let mut des = Deserializer::new(io::Cursor::new(&src[..]));

                let res = Deserialize::deserialize(&mut des).map_err(DecodeError::Decode);

                // check for unexpected EOF to indicate that we need more data
                if let Err(DecodeError::Decode(
                    decode::Error::InvalidDataRead(ref custom)
                    | decode::Error::InvalidMarkerRead(ref custom),
                )) = res
                    && custom.kind() == io::ErrorKind::UnexpectedEof
                {
                    // We need more bytes before a full message can be decoded. Bound
                    // the buffer so a peer cannot force unbounded memory growth by
                    // declaring a huge msgpack array/map/string and streaming the
                    // bytes: if the frame has already grown past the limit without
                    // yielding a complete message, drop the connection.
                    if src.len() > self.max_frame_size {
                        return Err(DecodeError::FrameTooLarge {
                            size: src.len(),
                            max: self.max_frame_size,
                        });
                    }
                    return Ok(None);
                }

                (des.position() as usize, res)
            };

            src.advance(byte_size);

            let maybe_item = self.handle_message(res, byte_size).inspect_err(|error| {
                let base64_encoded_message = BASE64_STANDARD.encode(&src[..]);
                emit!(FluentMessageDecodeError {
                    error,
                    base64_encoded_message
                });
            })?;
            if let Some(item) = maybe_item {
                return Ok(Some(item));
            }
        }
    }
}

/// Decoder for decoding MessagePackEventStream which are just a stream of Entries
#[derive(Clone, Debug)]
struct FluentEntryStreamDecoder;

impl Decoder for FluentEntryStreamDecoder {
    type Item = FluentEntry;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }
        let (byte_size, res) = {
            let mut des = Deserializer::new(io::Cursor::new(&src[..]));

            // attempt to parse, if we get unexpected EOF, we need more data
            let res = Deserialize::deserialize(&mut des).map_err(DecodeError::Decode);

            if let Err(DecodeError::Decode(decode::Error::InvalidDataRead(ref custom))) = res
                && custom.kind() == io::ErrorKind::UnexpectedEof
            {
                return Ok(None);
            }

            let byte_size = des.position();

            emit!(FluentMessageReceived { byte_size });

            (byte_size as usize, res)
        };

        src.advance(byte_size);

        res
    }
}

struct FluentAcker {
    chunks: Vec<String>,
}

impl FluentAcker {
    fn new(frames: &[FluentFrame]) -> Self {
        Self {
            chunks: frames.iter().filter_map(|f| f.chunk.clone()).collect(),
        }
    }
}

impl TcpSourceAcker for FluentAcker {
    fn build_ack(self, ack: TcpSourceAck) -> Option<Bytes> {
        if self.chunks.is_empty() {
            return None;
        }

        let mut buf = Vec::new();
        let mut ser = Serializer::new(&mut buf);
        let mut ack_map = HashMap::new();

        for chunk in self.chunks {
            ack_map.clear();
            if let TcpSourceAck::Ack = ack {
                ack_map.insert("ack", chunk);
            };
            ack_map.serialize(&mut ser).unwrap();
        }
        Some(buf.into())
    }
}

/// Normalized fluent message.
#[derive(Debug, PartialEq)]
struct FluentEvent<'a> {
    tag: FluentTag,
    timestamp: FluentTimestamp,
    record: FluentRecord,
    log_namespace: &'a LogNamespace,
}

impl From<FluentEvent<'_>> for Event {
    fn from(frame: FluentEvent) -> Event {
        LogEvent::from(frame).into()
    }
}

struct FluentFrame {
    events: SmallVec<[Event; 1]>,
    chunk: Option<String>,
}

impl From<FluentFrame> for SmallVec<[Event; 1]> {
    fn from(frame: FluentFrame) -> Self {
        frame.events
    }
}

impl From<FluentEvent<'_>> for LogEvent {
    fn from(frame: FluentEvent) -> LogEvent {
        let FluentEvent {
            tag,
            timestamp,
            record,
            log_namespace,
        } = frame;

        let mut log = LogEvent::default();

        log_namespace.insert_vector_metadata(
            &mut log,
            log_schema().source_type_key(),
            path!("source_type"),
            Bytes::from_static(FluentConfig::NAME.as_bytes()),
        );

        match log_namespace {
            LogNamespace::Vector => {
                log.insert(metadata_path!(FluentConfig::NAME, "timestamp"), timestamp);
                log.insert(metadata_path!("vector", "ingest_timestamp"), Utc::now());
            }
            LogNamespace::Legacy => {
                log.maybe_insert(log_schema().timestamp_key_target_path(), timestamp);
            }
        }

        log_namespace.insert_source_metadata(
            FluentConfig::NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!("tag"))),
            path!("tag"),
            tag,
        );

        for (key, value) in record.into_iter() {
            let value: Value = value.into();
            log_namespace.insert_source_metadata(
                FluentConfig::NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(key.as_str()))),
                path!("record", key.as_str()),
                value,
            );
        }
        log
    }
}

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "fluent-integration-tests"))]
mod integration_tests;
