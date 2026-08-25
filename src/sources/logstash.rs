use std::{
    collections::BTreeMap,
    convert::TryFrom,
    io,
    net::SocketAddr,
    num::{NonZeroU64, NonZeroUsize},
    time::Duration,
};

use bytes::{Buf, Bytes, BytesMut};
use smallvec::{SmallVec, smallvec};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Decoder;
use vector_lib::{
    codecs::{BytesDeserializerConfig, StreamDecodingError},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    ipallowlist::IpAllowlistConfig,
    lookup::{OwnedValuePath, event_path, metadata_path, owned_value_path, path},
    schema::Definition,
};
use vrl::value::{KeyString, Kind, kind::Collection};

use super::util::decompression::{
    CappedDecoder, max_decompressed_size_bytes, max_zlib_compressed_frame_size_bytes,
};
use super::util::net::{SocketListenAddr, TcpSource, TcpSourceAck, TcpSourceAcker};
use crate::{
    config::{
        DataType, GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig,
        SourceContext, SourceOutput, log_schema,
    },
    event::{Event, LogEvent, Value},
    serde::bool_or_struct,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsSourceConfig},
    types,
};

/// Configuration for the `logstash` source.
#[configurable_component(source("logstash", "Collect logs from a Logstash agent."))]
#[derive(Clone, Debug)]
pub struct LogstashConfig {
    #[configurable(derived)]
    address: SocketListenAddr,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    #[configurable(derived)]
    pub permit_origin: Option<IpAllowlistConfig>,

    #[configurable(derived)]
    tls: Option<TlsSourceConfig>,

    /// The size of the receive buffer used for each connection.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    receive_buffer_bytes: Option<usize>,

    /// The maximum number of TCP connections that are allowed at any given time.
    #[configurable(metadata(docs::type_unit = "connections"))]
    connection_limit: Option<u32>,

    /// The timeout, in seconds, before a TLS handshake is aborted if it has not completed.
    ///
    /// This bounds how long a connection can hold its slot against `connection_limit`
    /// before the TLS handshake finishes, protecting against clients that open a
    /// connection but never complete (or never start) a handshake.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    tls_handshake_timeout_secs: Option<NonZeroU64>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,
}

impl LogstashConfig {
    /// Builds the `schema::Definition` for this source using the provided `LogNamespace`.
    fn schema_definition(&self, log_namespace: LogNamespace) -> Definition {
        // `host_key` is only inserted if not present already.
        let host_key = log_schema()
            .host_key()
            .cloned()
            .map(LegacyKey::InsertIfEmpty);

        let tls_client_metadata_path = self
            .tls
            .as_ref()
            .and_then(|tls| tls.client_metadata_key.as_ref())
            .and_then(|k| k.path.clone())
            .map(LegacyKey::Overwrite);

        BytesDeserializerConfig
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                LogstashConfig::NAME,
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            )
            .with_source_metadata(
                LogstashConfig::NAME,
                host_key,
                &owned_value_path!("host"),
                Kind::bytes(),
                Some("host"),
            )
            .with_source_metadata(
                Self::NAME,
                tls_client_metadata_path,
                &owned_value_path!("tls_client_metadata"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
    }
}

impl Default for LogstashConfig {
    fn default() -> Self {
        Self {
            address: SocketListenAddr::SocketAddr("0.0.0.0:5044".parse().unwrap()),
            keepalive: None,
            permit_origin: None,
            tls: None,
            receive_buffer_bytes: None,
            acknowledgements: Default::default(),
            connection_limit: None,
            tls_handshake_timeout_secs: None,
            log_namespace: None,
        }
    }
}

impl GenerateConfig for LogstashConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(LogstashConfig::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "logstash")]
impl SourceConfig for LogstashConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let source = LogstashSource {
            timestamp_converter: types::Conversion::Timestamp(cx.globals.timezone()),
            legacy_host_key_path: log_schema().host_key().cloned(),
            log_namespace,
        };
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
            LogstashConfig::NAME,
            log_namespace,
        )
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        // There is a global and per-source `log_namespace` config.
        // The source config overrides the global setting and is merged here.
        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            self.schema_definition(global_log_namespace.merge(self.log_namespace)),
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.as_tcp_resource()]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
struct LogstashSource {
    timestamp_converter: types::Conversion,
    log_namespace: LogNamespace,
    legacy_host_key_path: Option<OwnedValuePath>,
}

impl TcpSource for LogstashSource {
    type Error = DecodeError;
    type Item = LogstashEventFrame;
    type Decoder = LogstashDecoder;
    type Acker = LogstashAcker;

    fn decoder(&self) -> Self::Decoder {
        LogstashDecoder::new()
    }

    fn handle_events(&self, events: &mut [Event], host: SocketAddr) {
        let now = chrono::Utc::now();
        for event in events {
            let log = event.as_mut_log();

            self.log_namespace.insert_vector_metadata(
                log,
                log_schema().source_type_key(),
                path!("source_type"),
                Bytes::from_static(LogstashConfig::NAME.as_bytes()),
            );

            let log_timestamp = log.get(event_path!("@timestamp")).and_then(|timestamp| {
                self.timestamp_converter
                    .convert::<Value>(timestamp.coerce_to_bytes())
                    .ok()
            });

            // Vector: always insert `ingest_timestamp`. Insert `timestamp` if found in event.
            //
            // Legacy: always insert the global log schema timestamp key- use timestamp from
            //         event if present, otherwise use ingest.
            match self.log_namespace {
                LogNamespace::Vector => {
                    if let Some(timestamp) = log_timestamp {
                        log.insert(metadata_path!(LogstashConfig::NAME, "timestamp"), timestamp);
                    }
                    log.insert(metadata_path!("vector", "ingest_timestamp"), now);
                }
                LogNamespace::Legacy => {
                    if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
                        log.insert(
                            timestamp_key,
                            log_timestamp.unwrap_or_else(|| Value::from(now)),
                        );
                    }
                }
            }

            self.log_namespace.insert_source_metadata(
                LogstashConfig::NAME,
                log,
                self.legacy_host_key_path
                    .as_ref()
                    .map(LegacyKey::InsertIfEmpty),
                path!("host"),
                host.ip().to_string(),
            );
        }
    }

    fn build_acker(&self, frames: &[Self::Item]) -> Self::Acker {
        LogstashAcker::new(frames)
    }
}

struct LogstashAcker {
    // One cumulative ACK per *completed* writer window in the batch, in wire
    // order. We ACK only frames that complete a window (`window_end`); we never
    // emit a partial ACK for a window that is only partially present in this
    // batch.
    //
    // A partial ACK would put a sequence number on the wire that falls inside a
    // window the client is still filling, rather than on the window boundary the
    // client waits for. (Real clients always send exactly `window_size` events
    // per window and wait for an ACK of that count; see the decoder's
    // `WindowSize` handling and logstash.md.) The upstream Logstash server never
    // emits such partial ACKs, and an intermediary (load balancer, service mesh)
    // that buffers and later misdelivers one onto a different connection causes
    // the client to reject it as `invalid sequence number received
    // (seq=N, expected=M)`. A window split across batches is ACKed only once its
    // final event arrives in a later batch. (A window can never be closed early
    // by a new `WindowSize`: the decoder rejects a premature `WindowSize` as a
    // fatal error, so every window the acker sees is either complete or a genuine
    // trailing tail.)
    acknowledgements: SmallVec<[(LogstashProtocolVersion, u32); 1]>,
}

impl LogstashAcker {
    fn new(frames: &[LogstashEventFrame]) -> Self {
        let acknowledgements = frames
            .iter()
            // ACK only completed writer windows; never a partial trailing tail.
            .filter(|frame| frame.window_end)
            .map(|frame| (frame.protocol, frame.sequence_number))
            .collect();

        Self { acknowledgements }
    }
}

impl TcpSourceAcker for LogstashAcker {
    // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#ack-frame-type
    fn build_ack(self, ack: TcpSourceAck) -> Option<Bytes> {
        match ack {
            TcpSourceAck::Ack if !self.acknowledgements.is_empty() => {
                let mut bytes: Vec<u8> = Vec::with_capacity(self.acknowledgements.len() * 6);
                for (protocol_version, sequence_number) in self.acknowledgements {
                    bytes.push(protocol_version.into());
                    bytes.push(LogstashFrameType::Ack.into());
                    bytes.extend(sequence_number.to_be_bytes().iter());
                }
                Some(Bytes::from(bytes))
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
enum LogstashDecoderReadState {
    ReadProtocol,
    ReadType(LogstashProtocolVersion),
    ReadFrame(LogstashProtocolVersion, LogstashFrameType),
    // A decompressed payload plus the nested decoder reading it. `Box` keeps the
    // type finite-sized.
    PendingDecompressed {
        buf: BytesMut,
        decoder: Box<LogstashDecoder>,
    },
}

#[derive(Debug)]
struct LogstashDecoder {
    state: LogstashDecoderReadState,
    // Tracks how many events remain in the current writer window. This lets us
    // preserve sender window boundaries even if ReadyFrames later batches
    // multiple decoded windows together before ACKing.
    window_events_remaining: Option<NonZeroUsize>,
    // Set for the decoder used to parse a decompressed payload. No known
    // Lumberjack/Beats client emits a compressed frame nested inside another,
    // so a nested `C` frame here is rejected rather than recursed into.
    // Without this, an attacker could nest compressed frames arbitrarily deep
    // and drive unbounded recursion in `decode_compressed_frame`, exhausting
    // the stack (CWE-674).
    nested: bool,
    // Maximum number of bytes buffered while waiting for a complete frame.
    // Bounds memory against a peer that declares an oversized JSON payload or
    // an absurd data-frame pair count and streams the bytes to force unbounded
    // buffering (mirrors fluent's `max_frame_size`).
    max_frame_size: usize,
}

impl LogstashDecoder {
    fn new() -> Self {
        Self {
            state: LogstashDecoderReadState::ReadProtocol,
            window_events_remaining: None,
            nested: false,
            max_frame_size: max_decompressed_size_bytes(),
        }
    }

    fn new_nested(window_events_remaining: Option<NonZeroUsize>) -> Self {
        Self {
            state: LogstashDecoderReadState::ReadProtocol,
            window_events_remaining,
            nested: true,
            max_frame_size: max_decompressed_size_bytes(),
        }
    }

    /// Marks whether a decoded frame closes the current writer window.
    ///
    /// Filebeat expects ACKs to stay within the current window announced by the
    /// most recent `WindowSize` frame. The generic TCP batching layer can merge
    /// frames from multiple windows before we build an ACK, so we record the
    /// per-frame window boundary here and let the acker emit one ACK frame per
    /// completed window later.
    ///
    /// If a sender omits `WindowSize`, we keep the previous behavior and treat
    /// each standalone frame as ACKable on its own.
    const fn annotate_frame(&mut self, frame: &mut LogstashEventFrame) {
        match self.window_events_remaining {
            Some(remaining) if remaining.get() == 1 => {
                frame.window_end = true;
                self.window_events_remaining = None;
            }
            Some(remaining) => {
                frame.window_end = false;
                self.window_events_remaining = NonZeroUsize::new(remaining.get() - 1); // safe because we know remaining is greater than 1
            }
            None => {
                // Preserve existing behavior for inputs that send standalone data frames
                // without an explicit WindowSize frame.
                frame.window_end = true;
            }
        }
    }
}

#[derive(Debug, Snafu)]
pub enum DecodeError {
    #[snafu(display("i/o error: {}", source))]
    IO { source: io::Error },
    #[snafu(display("Unknown logstash protocol version: {}", version))]
    UnknownProtocolVersion { version: char },
    #[snafu(display("Unknown logstash protocol message type: {}", frame_type))]
    UnknownFrameType { frame_type: char },
    #[snafu(display("Failed to decode JSON frame: {}", source))]
    JsonFrameFailedDecode { source: serde_json::Error },
    #[snafu(display("Failed to decompress compressed frame: {}", source))]
    DecompressionFailed { source: io::Error },
    #[snafu(display(
        "Received a WindowSize frame before the current window completed ({remaining} events still expected)"
    ))]
    PrematureWindowSize { remaining: usize },
    #[snafu(display("Compressed frame contains a nested compressed frame"))]
    NestedCompressedFrame,
    #[snafu(display(
        "logstash frame exceeds maximum size before decoding: {size} bytes buffered, limit is {max} bytes"
    ))]
    FrameTooLarge { size: usize, max: usize },
}

impl StreamDecodingError for DecodeError {
    fn can_continue(&self) -> bool {
        // No decode error is recoverable on this stream. Lumberjack is a
        // length-prefixed binary protocol with no resync marker, so once a
        // frame fails to decode the stream position is no longer trustworthy:
        // continuing would misframe subsequent bytes and emit ACKs for bogus
        // sequence numbers.
        false
    }
}

impl From<io::Error> for DecodeError {
    fn from(source: io::Error) -> Self {
        DecodeError::IO { source }
    }
}

#[derive(Debug, Clone, Copy)]
enum LogstashProtocolVersion {
    V1, // 1
    V2, // 2
}

impl From<LogstashProtocolVersion> for u8 {
    fn from(frame_type: LogstashProtocolVersion) -> u8 {
        use LogstashProtocolVersion::*;

        match frame_type {
            V1 => b'1',
            V2 => b'2',
        }
    }
}

impl TryFrom<u8> for LogstashProtocolVersion {
    type Error = DecodeError;

    fn try_from(frame_type: u8) -> Result<LogstashProtocolVersion, DecodeError> {
        use LogstashProtocolVersion::*;

        match frame_type {
            b'1' => Ok(V1),
            b'2' => Ok(V2),
            version => Err(DecodeError::UnknownProtocolVersion {
                version: version as char,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LogstashFrameType {
    Ack,        // A
    WindowSize, // W
    Data,       // D
    Json,       // J
    Compressed, // C
}

impl From<LogstashFrameType> for u8 {
    fn from(frame_type: LogstashFrameType) -> u8 {
        use LogstashFrameType::*;

        match frame_type {
            Ack => b'A',
            WindowSize => b'W',
            Data => b'D',
            Json => b'J',
            Compressed => b'C',
        }
    }
}

impl TryFrom<u8> for LogstashFrameType {
    type Error = DecodeError;

    fn try_from(frame_type: u8) -> Result<LogstashFrameType, DecodeError> {
        use LogstashFrameType::*;

        match frame_type {
            b'A' => Ok(Ack),
            b'W' => Ok(WindowSize),
            b'D' => Ok(Data),
            b'J' => Ok(Json),
            b'C' => Ok(Compressed),
            frame_type => Err(DecodeError::UnknownFrameType {
                frame_type: frame_type as char,
            }),
        }
    }
}

/// Normalized event from logstash frame
#[derive(Debug)]
struct LogstashEventFrame {
    protocol: LogstashProtocolVersion,
    sequence_number: u32,
    fields: BTreeMap<KeyString, serde_json::Value>,
    // True when this frame completes its window (fills it to the advertised
    // size). The acker emits one ACK per frame so marked.
    window_end: bool,
}

// Based on spec at: https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md
// And implementation from logstash: https://github.com/logstash-plugins/logstash-input-beats/blob/27bad62a26a81fc000a9d21495b8dc7174ab63e9/src/main/java/org/logstash/beats/BeatsParser.java
impl Decoder for LogstashDecoder {
    type Item = (LogstashEventFrame, usize);
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // This implements a sort of simple state machine to read the frames from the wire
        //
        // Each matched arm with either:
        // * Return that there is not enough data
        // * Return an error
        // * Read some bytes and advance the state
        loop {
            // An arm that returns `Ok(None)` leaves `self.state` untouched, so the next
            // `decode()` call resumes where this one left off: TCP can split a frame at
            // any byte boundary.
            self.state = match self.state {
                // Yield one inner frame per call so the downstream ReadyFrames batching
                // sees individual events instead of a fully expanded payload.
                LogstashDecoderReadState::PendingDecompressed {
                    ref mut buf,
                    ref mut decoder,
                } => match decoder.decode(buf)? {
                    Some(frame) => return Ok(Some(frame)),
                    None => {
                        // Payload exhausted: carry the window countdown back across the
                        // compression boundary before dropping the nested decoder.
                        self.window_events_remaining = decoder.window_events_remaining;
                        LogstashDecoderReadState::ReadProtocol
                    }
                },
                LogstashDecoderReadState::ReadProtocol => {
                    if src.remaining() < 1 {
                        return Ok(None);
                    }

                    use LogstashProtocolVersion::*;

                    match LogstashProtocolVersion::try_from(src.get_u8())? {
                        V1 => LogstashDecoderReadState::ReadType(V1),
                        V2 => LogstashDecoderReadState::ReadType(V2),
                    }
                }
                LogstashDecoderReadState::ReadType(protocol) => {
                    if src.remaining() < 1 {
                        return Ok(None);
                    }

                    use LogstashFrameType::*;

                    match LogstashFrameType::try_from(src.get_u8())? {
                        WindowSize => LogstashDecoderReadState::ReadFrame(protocol, WindowSize),
                        Data => LogstashDecoderReadState::ReadFrame(protocol, Data),
                        Json => LogstashDecoderReadState::ReadFrame(protocol, Json),
                        Compressed => LogstashDecoderReadState::ReadFrame(protocol, Compressed),
                        Ack => LogstashDecoderReadState::ReadFrame(protocol, Ack),
                    }
                }
                // The window size tells us how many events the writer will send
                // in this window before waiting for an ACK. We count those events
                // down (in `annotate_frame`) so the acker can mark the window's
                // boundary, even when ReadyFrames batches several windows together.
                //
                // The protocol spec defines window size as a *maximum* unacked
                // count, which read literally would let a writer underfill a
                // window and then open a new one. In practice every real client
                // (go-lumber, beats) sets the window size to the exact number of
                // events it then sends, and the reference go-lumber server treats
                // it as exact: its `readEvents` loop reads exactly `window_size`
                // frames and accepts only `J`/`D`/`C` frame bytes inside that loop,
                // so any other byte — including a premature `W` — hits the
                // `default` branch and returns `ErrProtocolError`, closing the
                // connection (`go-lumber/server/v2/reader.go:121-124`, v1:
                // `go-lumber/server/v1/reader.go:117-120`). We rely on that
                // observed behavior, not the looser spec wording. See the
                // "Window size" section of logstash.md for the client/server
                // references.
                //
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#window-size-frame-type
                LogstashDecoderReadState::ReadFrame(_protocol, LogstashFrameType::WindowSize) => {
                    // A new window must not open until the current one has received
                    // all advertised events. Because real clients always fill a
                    // window exactly (see above), a `WindowSize` arriving mid-window
                    // means the sender has desynced from the window contract; reject
                    // it as a fatal decode error (matching the reference server)
                    // rather than guess an ACK boundary for the abandoned window.
                    // Only a `WindowSize` can open a window, so it is the only frame
                    // that can prematurely close one. This guarantees every window
                    // the acker sees is either complete or a genuine trailing tail.
                    if let Some(remaining) = self.window_events_remaining {
                        return Err(DecodeError::PrematureWindowSize {
                            remaining: remaining.get(),
                        });
                    }

                    if src.remaining() < 4 {
                        return Ok(None);
                    }

                    let window_size = src.get_u32() as usize;
                    self.window_events_remaining = NonZeroUsize::new(window_size);

                    LogstashDecoderReadState::ReadProtocol
                }
                // we shouldn't receive acks from the writer, just skip
                //
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#ack-frame-type
                LogstashDecoderReadState::ReadFrame(_protocol, LogstashFrameType::Ack) => {
                    if src.remaining() < 4 {
                        return Ok(None);
                    }

                    let _sequence_number = src.get_u32();

                    LogstashDecoderReadState::ReadProtocol
                }
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#data-frame-type
                LogstashDecoderReadState::ReadFrame(protocol, LogstashFrameType::Data) => {
                    let Some((mut frame, byte_size)) = decode_data_frame(protocol, src) else {
                        // A D frame declares a pair count rather than a byte length, so
                        // bound what an in-progress frame may buffer: nothing follows an
                        // incomplete frame, so `remaining()` is exactly its bytes.
                        if src.remaining() > self.max_frame_size {
                            return Err(DecodeError::FrameTooLarge {
                                size: src.remaining(),
                                max: self.max_frame_size,
                            });
                        }
                        return Ok(None);
                    };
                    self.annotate_frame(&mut frame);

                    self.state = LogstashDecoderReadState::ReadProtocol;
                    return Ok(Some((frame, byte_size)));
                }
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#json-frame-type
                LogstashDecoderReadState::ReadFrame(protocol, LogstashFrameType::Json) => {
                    let Some((mut frame, byte_size)) =
                        decode_json_frame(protocol, src, self.max_frame_size)?
                    else {
                        return Ok(None);
                    };
                    self.annotate_frame(&mut frame);

                    self.state = LogstashDecoderReadState::ReadProtocol;
                    return Ok(Some((frame, byte_size)));
                }
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#compressed-frame-type
                //
                // The compressed payload is still part of the same logical Lumberjack stream, so
                // the nested decoder inherits the current window state and hands it back once the
                // payload is fully expanded. Re-annotating the emitted frames here would overwrite
                // any WindowSize boundaries that were established inside the compressed payload
                // and can also lose progress from a partially consumed outer window.
                LogstashDecoderReadState::ReadFrame(_protocol, LogstashFrameType::Compressed) => {
                    if self.nested {
                        return Err(DecodeError::NestedCompressedFrame);
                    }

                    let Some(buf) = decode_compressed_frame(src)? else {
                        return Ok(None);
                    };

                    LogstashDecoderReadState::PendingDecompressed {
                        buf,
                        decoder: Box::new(LogstashDecoder::new_nested(
                            self.window_events_remaining,
                        )),
                    }
                }
            };
        }
    }
}

/// Decode the Lumberjack version 1 protocol, which use the Key:Value format.
fn decode_data_frame(
    protocol: LogstashProtocolVersion,
    src: &mut BytesMut,
) -> Option<(LogstashEventFrame, usize)> {
    let mut rest = src.as_ref();

    if rest.remaining() < 8 {
        return None;
    }
    let sequence_number = rest.get_u32();
    let pair_count = rest.get_u32();
    if pair_count == 0 {
        return None; // Invalid number of fields
    }

    let mut fields = BTreeMap::<KeyString, serde_json::Value>::new();
    for _ in 0..pair_count {
        let (key, value, right) = decode_pair(rest)?;
        rest = right;

        fields.insert(
            String::from_utf8_lossy(key).into(),
            String::from_utf8_lossy(value).into(),
        );
    }

    let byte_size = bytes_remaining(src, rest);
    src.advance(byte_size);

    Some((
        LogstashEventFrame {
            protocol,
            sequence_number,
            fields,
            window_end: false,
        },
        byte_size,
    ))
}

fn decode_pair(mut rest: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    if rest.remaining() < 4 {
        return None;
    }
    let key_length = rest.get_u32() as usize;

    if rest.remaining() < key_length {
        return None;
    }
    let (key, right) = rest.split_at(key_length);
    rest = right;

    if rest.remaining() < 4 {
        return None;
    }
    let value_length = rest.get_u32() as usize;
    if rest.remaining() < value_length {
        return None;
    }
    let (value, right) = rest.split_at(value_length);
    Some((key, value, right))
}

fn decode_json_frame(
    protocol: LogstashProtocolVersion,
    src: &mut BytesMut,
    max_frame_size: usize,
) -> Result<Option<(LogstashEventFrame, usize)>, DecodeError> {
    let mut rest = src.as_ref();

    if rest.remaining() < 8 {
        return Ok(None);
    }
    let sequence_number = rest.get_u32();
    let payload_size = rest.get_u32() as usize;

    // Reject an oversized declared payload before buffering it, so a peer cannot
    // force multi-GB buffering by advertising a huge length and slow-streaming
    // its bytes. Same bound as the compressed frame and fluent's max_frame_size.
    if payload_size > max_frame_size {
        return Err(DecodeError::FrameTooLarge {
            size: payload_size,
            max: max_frame_size,
        });
    }

    if rest.remaining() < payload_size {
        return Ok(None);
    }

    let (slice, right) = rest.split_at(payload_size);
    rest = right;

    let fields: BTreeMap<KeyString, serde_json::Value> =
        serde_json::from_slice(slice).context(JsonFrameFailedDecodeSnafu {})?;

    let byte_size = bytes_remaining(src, rest);
    src.advance(byte_size);

    Ok(Some((
        LogstashEventFrame {
            protocol,
            sequence_number,
            fields,
            window_end: false,
        },
        byte_size,
    )))
}

/// Decompresses a `C` frame's payload, leaving it to the caller to expand incrementally.
fn decode_compressed_frame(src: &mut BytesMut) -> Result<Option<BytesMut>, DecodeError> {
    let mut rest = src.as_ref();

    if rest.remaining() < 4 {
        return Ok(None);
    }
    let payload_size = rest.get_u32() as usize;
    let limit = max_decompressed_size_bytes();

    // Reject an oversized declared payload before buffering it, so a peer cannot force multi-GB
    // buffering by advertising a huge length and slow-streaming its bytes. The bound includes
    // zlib's worst-case expansion so a valid frame whose decompressed content is within `limit`
    // is never rejected here; the decompressed cap itself is still enforced below.
    let compressed_limit = max_zlib_compressed_frame_size_bytes();
    if payload_size > compressed_limit {
        return Err(DecodeError::DecompressionFailed {
            source: io::Error::other(format!(
                "compressed frame payload size {payload_size} exceeds limit of {compressed_limit} bytes"
            )),
        });
    }

    if rest.remaining() < payload_size {
        return Ok(None);
    }

    let (slice, right) = rest.split_at(payload_size);
    rest = right;

    let res = CappedDecoder::zlib_with_limit(io::Cursor::new(slice), limit)
        .decompress()
        .map(|v| BytesMut::from(v.as_slice()))
        .context(DecompressionFailedSnafu);

    let byte_size = bytes_remaining(src, rest);
    src.advance(byte_size);

    Ok(Some(res?))
}

fn bytes_remaining(src: &BytesMut, rest: &[u8]) -> usize {
    let remaining = rest.remaining();
    src.remaining() - remaining
}

impl From<LogstashEventFrame> for Event {
    fn from(frame: LogstashEventFrame) -> Self {
        Event::Log(LogEvent::from(
            frame
                .fields
                .into_iter()
                .map(|(key, value)| (key, Value::from(value)))
                .collect::<BTreeMap<_, _>>(),
        ))
    }
}

impl From<LogstashEventFrame> for SmallVec<[Event; 1]> {
    fn from(frame: LogstashEventFrame) -> Self {
        smallvec![frame.into()]
    }
}

#[cfg(test)]
mod test;

#[cfg(all(test, feature = "logstash-integration-tests"))]
mod integration_tests;
