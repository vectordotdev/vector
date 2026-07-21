use std::{
    collections::{BTreeMap, VecDeque},
    convert::TryFrom,
    io,
    net::SocketAddr,
    num::NonZeroUsize,
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
    #[configurable(metadata(docs::advanced))]
    keepalive: Option<TcpKeepaliveConfig>,

    #[configurable(derived)]
    pub permit_origin: Option<IpAllowlistConfig>,

    #[configurable(derived)]
    tls: Option<TlsSourceConfig>,

    /// The size of the receive buffer used for each connection.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    #[configurable(metadata(docs::advanced))]
    receive_buffer_bytes: Option<usize>,

    /// The maximum number of TCP connections that are allowed at any given time.
    #[configurable(metadata(docs::type_unit = "connections"))]
    #[configurable(metadata(docs::advanced))]
    connection_limit: Option<u32>,

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
            log_namespace: None,
        }
    }
}

impl GenerateConfig for LogstashConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(LogstashConfig::default()).unwrap()
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
    PendingFrames(VecDeque<(LogstashEventFrame, usize)>),
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
}

impl LogstashDecoder {
    const fn new() -> Self {
        Self::new_with_window_events_remaining(None)
    }

    const fn new_with_window_events_remaining(
        window_events_remaining: Option<NonZeroUsize>,
    ) -> Self {
        Self {
            state: LogstashDecoderReadState::ReadProtocol,
            window_events_remaining,
            nested: false,
        }
    }

    const fn new_nested(window_events_remaining: Option<NonZeroUsize>) -> Self {
        Self {
            state: LogstashDecoderReadState::ReadProtocol,
            window_events_remaining,
            nested: true,
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

struct DecodedCompressedFrames {
    frames: VecDeque<(LogstashEventFrame, usize)>,
    window_events_remaining: Option<NonZeroUsize>,
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
            self.state = match self.state {
                // if we have any unsent frames, send them before reading new logstash frame
                LogstashDecoderReadState::PendingFrames(ref mut frames) => {
                    match frames.pop_front() {
                        Some(frame) => return Ok(Some(frame)),
                        None => LogstashDecoderReadState::ReadProtocol,
                    }
                }
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
                        return Ok(None);
                    };
                    self.annotate_frame(&mut frame);

                    LogstashDecoderReadState::PendingFrames([(frame, byte_size)].into())
                }
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#json-frame-type
                LogstashDecoderReadState::ReadFrame(protocol, LogstashFrameType::Json) => {
                    let Some((mut frame, byte_size)) = decode_json_frame(protocol, src)? else {
                        return Ok(None);
                    };
                    self.annotate_frame(&mut frame);

                    LogstashDecoderReadState::PendingFrames([(frame, byte_size)].into())
                }
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#compressed-frame-type
                //
                // The compressed payload is still part of the same logical Lumberjack stream, so
                // the nested decoder must inherit the current window state and return the updated
                // state after expanding the payload. Re-annotating the emitted frames here would
                // overwrite any WindowSize boundaries that were established inside the compressed
                // payload and can also lose progress from a partially consumed outer window.
                LogstashDecoderReadState::ReadFrame(_protocol, LogstashFrameType::Compressed) => {
                    if self.nested {
                        return Err(DecodeError::NestedCompressedFrame);
                    }

                    let Some(decoded) = decode_compressed_frame(src, self.window_events_remaining)?
                    else {
                        return Ok(None);
                    };
                    self.window_events_remaining = decoded.window_events_remaining;

                    LogstashDecoderReadState::PendingFrames(decoded.frames)
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
) -> Result<Option<(LogstashEventFrame, usize)>, DecodeError> {
    let mut rest = src.as_ref();

    if rest.remaining() < 8 {
        return Ok(None);
    }
    let sequence_number = rest.get_u32();
    let payload_size = rest.get_u32() as usize;

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

fn decode_compressed_frame(
    src: &mut BytesMut,
    window_events_remaining: Option<NonZeroUsize>,
) -> Result<Option<DecodedCompressedFrames>, DecodeError> {
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

    let mut buf = res?;

    let mut decoder = LogstashDecoder::new_nested(window_events_remaining);

    let mut frames = VecDeque::new();

    while let Some(s) = decoder.decode(&mut buf)? {
        frames.push_back(s);
    }
    Ok(Some(DecodedCompressedFrames {
        frames,
        window_events_remaining: decoder.window_events_remaining,
    }))
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
mod test {
    use std::io::Write;

    use bytes::BufMut;
    use flate2::{Compression, write::ZlibEncoder};
    use futures::{Stream, StreamExt, stream};
    use rand::{Rng, rng};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use vector_lib::codecs::ReadyFrames;
    use vector_lib::lookup::OwnedTargetPath;
    use vrl::event_path;
    use vrl::value::kind::Collection;

    use super::*;
    use crate::{
        SourceSender,
        event::EventStatus,
        test_util::{
            addr::next_addr,
            components::{SOCKET_PUSH_SOURCE_TAGS, assert_source_compliance},
            spawn_collect_n, wait_for_tcp,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<LogstashConfig>();
    }

    #[tokio::test]
    async fn test_delivered() {
        test_protocol(EventStatus::Delivered, true).await;
    }

    #[tokio::test]
    async fn test_failed() {
        test_protocol(EventStatus::Rejected, false).await;
    }

    async fn start_logstash(
        status: EventStatus,
    ) -> (SocketAddr, impl Stream<Item = Event> + Unpin) {
        let (sender, recv) = SourceSender::new_test_finalize(status);
        let (_guard, address) = next_addr();
        let source = LogstashConfig {
            address: address.into(),
            tls: None,
            permit_origin: None,
            keepalive: None,
            receive_buffer_bytes: None,
            acknowledgements: true.into(),
            connection_limit: None,
            log_namespace: None,
        }
        .build(SourceContext::new_test(sender, None))
        .await
        .unwrap();
        tokio::spawn(source);
        wait_for_tcp(address).await;
        (address, recv)
    }

    async fn test_protocol(status: EventStatus, sends_ack: bool) {
        let events = assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let (address, recv) = start_logstash(status).await;
            spawn_collect_n(
                send_req(address, &[("message", "Hello, world!")], sends_ack),
                recv,
                1,
            )
            .await
        })
        .await;

        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert_eq!(
            log.get(event_path!("message")).unwrap().to_string_lossy(),
            "Hello, world!".to_string()
        );
        assert_eq!(
            log.get(event_path!("source_type"))
                .unwrap()
                .to_string_lossy(),
            "logstash".to_string()
        );
        assert!(log.get(event_path!("host")).is_some());
        assert!(log.get(event_path!("timestamp")).is_some());
    }

    fn push_req(req: &mut BytesMut, seq: u32, pairs: &[(&str, &str)]) {
        req.put_u8(b'2');
        req.put_u8(b'D');
        req.put_u32(seq);
        req.put_u32(pairs.len() as u32);
        for (key, value) in pairs {
            req.put_u32(key.len() as u32);
            req.put(key.as_bytes());
            req.put_u32(value.len() as u32);
            req.put(value.as_bytes());
        }
    }

    fn encode_req(seq: u32, pairs: &[(&str, &str)]) -> Bytes {
        let mut req = BytesMut::new();
        push_req(&mut req, seq, pairs);
        req.into()
    }

    fn push_window_size(req: &mut BytesMut, size: u32) {
        req.put_u8(b'2');
        req.put_u8(b'W');
        req.put_u32(size);
    }

    fn push_compressed(req: &mut BytesMut, inner: &[u8]) {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(inner).unwrap();
        let compressed = encoder.finish().unwrap();

        req.put_u8(b'2');
        req.put_u8(b'C');
        req.put_u32(compressed.len() as u32);
        req.put(compressed.as_slice());
    }

    fn decode_frames(mut src: BytesMut) -> Vec<(LogstashEventFrame, usize)> {
        let mut decoder = LogstashDecoder::new();
        let mut frames = Vec::new();

        while let Some(frame) = decoder.decode(&mut src).unwrap() {
            frames.push(frame);
        }

        assert_eq!(src.len(), 0);
        frames
    }

    fn decode_acknowledgements(mut ack: Bytes) -> Vec<u32> {
        let mut acknowledgements = Vec::new();

        while !ack.is_empty() {
            assert!(
                ack.len() >= 6,
                "ack stream ended with {} trailing bytes",
                ack.len()
            );
            assert_eq!(ack.get_u8(), b'2');
            assert_eq!(ack.get_u8(), b'A');
            acknowledgements.push(ack.get_u32());
        }

        acknowledgements
    }

    fn decoded_sequence_numbers(decoded: &[(LogstashEventFrame, usize)]) -> Vec<u32> {
        decoded
            .iter()
            .map(|(frame, _)| frame.sequence_number)
            .collect::<Vec<_>>()
    }

    fn assert_decoded_sequences(
        decoded: &[(LogstashEventFrame, usize)],
        expected_sequences: &[u32],
    ) {
        assert_eq!(decoded_sequence_numbers(decoded), expected_sequences);
    }

    async fn assert_acknowledgements_for_ready_frames(
        decoded: Vec<(LogstashEventFrame, usize)>,
        expected_sequences: &[u32],
        expected_acknowledgements: &[u32],
    ) {
        assert_decoded_sequences(&decoded, expected_sequences);

        let stream = stream::iter(decoded.into_iter().map(Ok::<_, DecodeError>));
        let mut ready = ReadyFrames::with_capacity(stream, 16);
        let (frames, _) = ready.next().await.unwrap().unwrap();

        // An incomplete window produces no ACK at all (`build_ack` returns
        // `None`); treat that as an empty acknowledgement list so callers can
        // assert it with `&[]`.
        let acknowledgements = LogstashAcker::new(&frames)
            .build_ack(TcpSourceAck::Ack)
            .map_or_else(Vec::new, decode_acknowledgements);

        assert!(ready.next().await.is_none());
        assert_eq!(acknowledgements, expected_acknowledgements);
    }

    fn decode_frames_and_assert_sequences(
        src: BytesMut,
        expected_sequences: &[u32],
    ) -> Vec<(LogstashEventFrame, usize)> {
        let decoded = decode_frames(src);
        assert_decoded_sequences(&decoded, expected_sequences);
        decoded
    }

    fn decode_frames_with_decoder(
        decoder: &mut LogstashDecoder,
        mut src: BytesMut,
    ) -> Vec<(LogstashEventFrame, usize)> {
        let mut frames = Vec::new();

        while let Some(frame) = decoder.decode(&mut src).unwrap() {
            frames.push(frame);
        }

        assert_eq!(src.len(), 0);
        frames
    }

    fn decode_frames_with_decoder_and_assert_sequences(
        decoder: &mut LogstashDecoder,
        src: BytesMut,
        expected_sequences: &[u32],
    ) -> Vec<(LogstashEventFrame, usize)> {
        let decoded = decode_frames_with_decoder(decoder, src);
        assert_decoded_sequences(&decoded, expected_sequences);
        decoded
    }

    #[test]
    fn v1_decoder_does_not_panic() {
        let seq = rng().random_range(1..u32::MAX);
        let req = encode_req(seq, &[("message", "Hello, World!")]);
        for i in 0..req.len() - 1 {
            assert!(
                decode_data_frame(LogstashProtocolVersion::V1, &mut BytesMut::from(&req[..i]))
                    .is_none()
            );
        }
    }

    // A malformed frame must be a fatal (non-continuable) decode error: the
    // Lumberjack stream can't be resynced, so the connection is closed rather
    // than continuing with a desynced decoder (which would emit bogus ACKs).
    // This matches upstream logstash-input-beats, which closes the channel on
    // any decode exception.

    #[test]
    fn malformed_json_frame_is_a_fatal_decode_error() {
        let mut decoder = LogstashDecoder::new();
        let mut src = BytesMut::new();
        src.put_u8(b'2');
        src.put_u8(b'J');
        src.put_u32(1); // sequence number
        let bad = b"{ not valid json ";
        src.put_u32(bad.len() as u32); // payload size
        src.put(&bad[..]);

        let err = decoder.decode(&mut src).unwrap_err();
        assert!(matches!(err, DecodeError::JsonFrameFailedDecode { .. }));
        assert!(
            !err.can_continue(),
            "a malformed JSON frame must be fatal so the connection closes",
        );
    }

    #[test]
    fn malformed_compressed_frame_is_a_fatal_decode_error() {
        let mut decoder = LogstashDecoder::new();
        let mut src = BytesMut::new();
        src.put_u8(b'2');
        src.put_u8(b'C');
        let garbage = b"this is not a zlib stream";
        src.put_u32(garbage.len() as u32); // payload size
        src.put(&garbage[..]);

        let err = decoder.decode(&mut src).unwrap_err();
        assert!(matches!(err, DecodeError::DecompressionFailed { .. }));
        assert!(!err.can_continue());
    }

    #[test]
    fn premature_window_size_frame_is_a_fatal_decode_error() {
        // A WindowSize frame that arrives before the current window has received
        // all its advertised events is a protocol violation, matching the
        // upstream Logstash/go-lumber server. The reference server's `readEvents`
        // loop reads exactly `window_size` frames and only accepts `J`/`D`/`C`
        // frame bytes inside that loop; any other byte — including a premature
        // `W` — hits its `default` branch and returns `ErrProtocolError`, closing
        // the connection (`go-lumber/server/v2/reader.go:121-124`). Every real
        // client (go-lumber, beats) sends exactly `window_size` events per window,
        // so a mid-window `WindowSize` means the sender has desynced; see the
        // "Window size" section of logstash.md. Rejecting it here means an
        // under-filled window can never reach the acker, so the acker never has to
        // invent a boundary for a window the sender closed early.
        let mut decoder = LogstashDecoder::new();
        let mut src = BytesMut::new();
        push_window_size(&mut src, 2);
        push_req(&mut src, 1, &[("message", "only one of two")]);
        push_window_size(&mut src, 5); // premature: window 1 still expects another event

        // The first decode yields the single data frame of the incomplete window.
        assert!(decoder.decode(&mut src).unwrap().is_some());

        // The next decode reaches the premature WindowSize and fails fatally so
        // the connection is closed rather than silently re-framed.
        let err = decoder.decode(&mut src).unwrap_err();
        assert!(matches!(err, DecodeError::PrematureWindowSize { .. }));
        assert!(
            !err.can_continue(),
            "a premature WindowSize must be fatal so the connection closes",
        );
    }

    #[test]
    fn premature_window_size_inside_compressed_payload_is_fatal() {
        // The premature-WindowSize guard also fires inside a compressed payload:
        // the nested decoder run by `decode_compressed_frame` hits the guard, and
        // the error propagates out through the `?` on the nested decode loop so
        // the outer decode returns it. This locks in that error propagation.
        let mut inner = BytesMut::new();
        push_window_size(&mut inner, 2);
        push_req(&mut inner, 1, &[("message", "only one of two")]);
        push_window_size(&mut inner, 5); // premature, inside the compressed payload

        let mut req = BytesMut::new();
        push_compressed(&mut req, &inner);

        let mut decoder = LogstashDecoder::new();
        let err = decoder.decode(&mut req).unwrap_err();
        assert!(matches!(err, DecodeError::PrematureWindowSize { .. }));
        assert!(
            !err.can_continue(),
            "a premature WindowSize inside a compressed frame must be fatal",
        );
    }

    #[test]
    fn nested_compressed_frame_is_a_fatal_decode_error() {
        let mut inner = BytesMut::new();
        push_req(&mut inner, 1, &[("message", "should never be reached")]);

        let mut middle = BytesMut::new();
        push_compressed(&mut middle, &inner);

        let mut req = BytesMut::new();
        push_compressed(&mut req, &middle);

        let mut decoder = LogstashDecoder::new();
        let err = decoder.decode(&mut req).unwrap_err();
        assert!(matches!(err, DecodeError::NestedCompressedFrame));
        assert!(!err.can_continue());
    }

    #[tokio::test]
    async fn malformed_frame_closes_connection_without_ack() {
        let (address, _recv) = start_logstash(EventStatus::Delivered).await;

        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();

        // A '2' 'J' frame whose payload is not valid JSON.
        let mut req = BytesMut::new();
        req.put_u8(b'2');
        req.put_u8(b'J');
        req.put_u32(1); // sequence number
        let bad = b"{ not valid json ";
        req.put_u32(bad.len() as u32); // payload size
        req.put(&bad[..]);
        socket.write_all(&req).await.unwrap();

        // The source must close the connection on the decode error and send no
        // ACK; the client will reconnect and retransmit.
        let mut output = BytesMut::new();
        let result = socket.read_buf(&mut output).await;
        assert!(
            matches!(result, Ok(0)) || result.is_err(),
            "expected the connection to close; read returned {result:?} with {output:?}",
        );
        assert!(
            output.is_empty(),
            "no ACK should be sent for a malformed frame, got {output:?}",
        );
    }

    #[tokio::test]
    async fn distinct_windows_do_not_share_an_ack_domain() {
        let mut req = BytesMut::new();
        push_window_size(&mut req, 1);
        push_req(&mut req, 1, &[("message", "first window")]);
        push_window_size(&mut req, 2);
        push_req(&mut req, 1, &[("message", "second window first")]);
        push_req(&mut req, 2, &[("message", "second window second")]);

        let decoded = decode_frames_and_assert_sequences(req, &[1, 1, 2]);
        assert_acknowledgements_for_ready_frames(decoded, &[1, 1, 2], &[1, 2]).await;
    }

    #[tokio::test]
    async fn distinct_windows_with_monotonic_sequences_ack_the_first_window() {
        let mut req = BytesMut::new();
        push_window_size(&mut req, 2);
        push_req(&mut req, 1, &[("message", "first window first")]);
        push_req(&mut req, 2, &[("message", "first window second")]);
        push_window_size(&mut req, 2);
        push_req(&mut req, 3, &[("message", "second window first")]);
        push_req(&mut req, 4, &[("message", "second window second")]);

        let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 3, 4]);
        assert_acknowledgements_for_ready_frames(decoded, &[1, 2, 3, 4], &[2, 4]).await;
    }

    #[tokio::test]
    async fn incomplete_window_is_not_acked() {
        // A window that has not yet received all its events must not be ACKed.
        // Emitting a partial ACK here puts a sequence number on the wire that
        // does not correspond to any window boundary the client declared; if
        // that ACK is later misattributed to a different (smaller) window by an
        // intermediary, the client rejects it as `invalid sequence number
        // received`. Matching the upstream Logstash server, we only ACK once
        // the window's final event arrives.
        let mut req = BytesMut::new();
        push_window_size(&mut req, 4);
        push_req(&mut req, 1, &[("message", "only event in partial window")]);

        let decoded = decode_frames_and_assert_sequences(req, &[1]);
        assert_acknowledgements_for_ready_frames(decoded, &[1], &[]).await;
    }

    #[tokio::test]
    async fn window_split_across_compressed_frames_acks_once_on_completion() {
        // A single window whose advertised size exceeds the number of events in
        // any one compressed frame is split across several compressed frames. The
        // decoder must thread `window_events_remaining` out of each
        // `decode_compressed_frame` so the countdown continues across the
        // compression boundary: only the window's true final event is marked
        // `window_end`, yielding exactly one ACK. A threading bug would either
        // mark a spurious boundary inside an earlier compressed frame (two
        // window_ends -> two ACKs) or never complete the window at all.
        let mut first = BytesMut::new();
        push_req(&mut first, 1, &[("message", "w4 first")]);
        push_req(&mut first, 2, &[("message", "w4 second")]);

        let mut second = BytesMut::new();
        push_req(&mut second, 3, &[("message", "w4 third")]);
        push_req(&mut second, 4, &[("message", "w4 fourth")]);

        // WindowSize(4) is sent uncompressed (as beats does), then the four
        // events arrive two-per-compressed-frame, so each compressed frame
        // carries fewer events than the window size.
        let mut req = BytesMut::new();
        push_window_size(&mut req, 4);
        push_compressed(&mut req, &first);
        push_compressed(&mut req, &second);

        let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 3, 4]);
        assert_acknowledgements_for_ready_frames(decoded, &[1, 2, 3, 4], &[4]).await;
    }

    #[tokio::test]
    async fn window_larger_than_ready_frames_capacity_in_one_compressed_frame_acks_once() {
        const WINDOW: u32 = 5;
        const CAPACITY: usize = 2;

        let mut inner = BytesMut::new();
        for seq in 1..=WINDOW {
            push_req(&mut inner, seq, &[("message", "event in oversized window")]);
        }

        // A following small window. Its sequence numbers restart at 1 (per the
        // protocol), and crucially its first event will share a ReadyFrames batch
        // with the oversized window's completing event (seq 5). That batch must
        // ACK only the oversized window (seq 5); the small window's retained first
        // event must NOT produce a second ACK until the small window's own final
        // event arrives.
        const SMALL_WINDOW: u32 = 2;
        let mut small_inner = BytesMut::new();
        for seq in 1..=SMALL_WINDOW {
            push_req(
                &mut small_inner,
                seq,
                &[("message", "event in small window")],
            );
        }

        // WindowSize is sent uncompressed (as beats does), then each whole window
        // arrives in a single compressed frame, exactly as in the report.
        let mut req = BytesMut::new();
        push_window_size(&mut req, WINDOW);
        push_compressed(&mut req, &inner);
        push_window_size(&mut req, SMALL_WINDOW);
        push_compressed(&mut req, &small_inner);

        let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 3, 4, 5, 1, 2]);

        let stream = stream::iter(decoded.into_iter().map(Ok::<_, DecodeError>));
        let mut ready = ReadyFrames::with_capacity(stream, CAPACITY);
        let mut acknowledgements = Vec::new();

        while let Some(result) = ready.next().await {
            let (frames, _byte_size) = result.unwrap();
            let acks = LogstashAcker::new(&frames)
                .build_ack(TcpSourceAck::Ack)
                .map_or_else(Vec::new, decode_acknowledgements);
            acknowledgements.push(acks);
        }

        // Batches: [1, 2] [3, 4] [5, 1] [2].
        // - The oversized window completes in the [5, 1] batch -> ACK(5) only; the
        //   retained small-window seq 1 in that same batch is NOT a window end, so
        //   it produces no ACK.
        // - The small window completes in the [2] batch -> ACK(2).
        // Each window is therefore ACKed exactly once; the oversized window's
        // sequence number (5) never reappears in a later batch.
        assert_eq!(acknowledgements, vec![vec![], vec![], vec![5], vec![2]]);

        // Each window is ACKed exactly once: seq 5 (oversized) and seq 2 (small).
        let all_acks: Vec<u32> = acknowledgements.into_iter().flatten().collect();
        assert_eq!(
            all_acks,
            vec![5, 2],
            "each window must be ACKed exactly once"
        );
    }

    #[tokio::test]
    async fn complete_window_then_incomplete_window_acks_only_the_complete_one() {
        // The customer-reported failure: a small complete window followed by a
        // larger window that is only partially present in the same batch. The
        // acker must emit only the completed window's ACK and must not emit a
        // partial ACK for the incomplete trailing window, whose sequence number
        // would otherwise exceed the smaller window the client awaits.
        let mut req = BytesMut::new();
        push_window_size(&mut req, 2);
        push_req(&mut req, 1, &[("message", "complete window first")]);
        push_req(&mut req, 2, &[("message", "complete window second")]);
        push_window_size(&mut req, 1000);
        push_req(&mut req, 1, &[("message", "partial window first")]);
        push_req(&mut req, 2, &[("message", "partial window second")]);
        push_req(&mut req, 3, &[("message", "partial window third")]);

        let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 1, 2, 3]);
        assert_acknowledgements_for_ready_frames(decoded, &[1, 2, 1, 2, 3], &[2]).await;
    }

    #[tokio::test]
    async fn compressed_frames_preserve_inner_window_boundaries() {
        let mut inner = BytesMut::new();
        push_window_size(&mut inner, 2);
        push_req(&mut inner, 1, &[("message", "compressed first")]);
        push_req(&mut inner, 2, &[("message", "compressed second")]);

        let mut req = BytesMut::new();
        push_compressed(&mut req, &inner);

        let decoded = decode_frames_and_assert_sequences(req, &[1, 2]);
        assert_acknowledgements_for_ready_frames(decoded, &[1, 2], &[2]).await;
    }

    #[tokio::test]
    async fn single_window_split_across_ready_frames_acks_only_on_completion() {
        // When one writer window is split across multiple `ReadyFrames` batches,
        // only the batch containing the window's final event (its `window_end`)
        // produces an ACK. Earlier batches hold no window boundary, so they emit
        // nothing rather than a partial ACK for a mid-window sequence number.
        let mut req = BytesMut::new();
        push_window_size(&mut req, 4);
        push_req(&mut req, 1, &[("message", "first")]);
        push_req(&mut req, 2, &[("message", "second")]);
        push_req(&mut req, 3, &[("message", "third")]);
        push_req(&mut req, 4, &[("message", "fourth")]);

        let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 3, 4]);

        let stream = stream::iter(decoded.into_iter().map(Ok::<_, DecodeError>));
        let mut ready = ReadyFrames::with_capacity(stream, 2);
        let mut acknowledgements = Vec::new();

        while let Some(result) = ready.next().await {
            let (frames, _byte_size) = result.unwrap();
            let acks = LogstashAcker::new(&frames)
                .build_ack(TcpSourceAck::Ack)
                .map_or_else(Vec::new, decode_acknowledgements);
            acknowledgements.push(acks);
        }

        // First batch (seq 1, 2) holds no window boundary -> no ACK.
        // Second batch (seq 3, 4) completes the window -> ACK(4).
        assert_eq!(acknowledgements, vec![vec![], vec![4]]);
    }

    #[tokio::test]
    async fn fresh_window_after_completed_window_is_accepted() {
        // A decoder reused across reads accepts a fresh window once the previous
        // window has completed. This is the legitimate counterpart to the
        // premature-WindowSize error: a `WindowSize` is only rejected when it
        // arrives mid-window, so opening a new window after the prior one has
        // received all its advertised events is always allowed. (A `WindowSize`
        // after a still-partial window would now be a fatal protocol error; see
        // `premature_window_size_frame_is_a_fatal_decode_error`.)
        let mut decoder = LogstashDecoder::new();

        let mut first_batch = BytesMut::new();
        push_window_size(&mut first_batch, 1);
        push_req(&mut first_batch, 1, &[("message", "first window")]);
        let decoded =
            decode_frames_with_decoder_and_assert_sequences(&mut decoder, first_batch, &[1]);
        assert_acknowledgements_for_ready_frames(decoded, &[1], &[1]).await;

        let mut second_batch = BytesMut::new();
        push_window_size(&mut second_batch, 1);
        push_req(
            &mut second_batch,
            1,
            &[("message", "fresh window after completion")],
        );
        let decoded =
            decode_frames_with_decoder_and_assert_sequences(&mut decoder, second_batch, &[1]);
        assert_acknowledgements_for_ready_frames(decoded, &[1], &[1]).await;
    }

    async fn send_req(address: SocketAddr, pairs: &[(&str, &str)], sends_ack: bool) {
        let seq = rng().random_range(1..u32::MAX);
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();

        let req = encode_req(seq, pairs);
        socket.write_all(&req).await.unwrap();

        let mut output = BytesMut::new();
        socket.read_buf(&mut output).await.unwrap();

        if sends_ack {
            assert_eq!(output.get_u8(), b'2');
            assert_eq!(output.get_u8(), b'A');
            assert_eq!(output.get_u32(), seq);
        }
        assert_eq!(output.len(), 0);
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = LogstashConfig {
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
                    &owned_value_path!(LogstashConfig::NAME, "timestamp"),
                    Kind::timestamp().or_undefined(),
                    Some("timestamp"),
                )
                .with_metadata_field(
                    &owned_value_path!(LogstashConfig::NAME, "host"),
                    Kind::bytes(),
                    Some("host"),
                )
                .with_metadata_field(
                    &owned_value_path!(LogstashConfig::NAME, "tls_client_metadata"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None,
                );

        assert_eq!(definitions, Some(expected_definition))
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = LogstashConfig::default();

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
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
        .with_event_field(&owned_value_path!("host"), Kind::bytes(), Some("host"));

        assert_eq!(definitions, Some(expected_definition))
    }
}

#[cfg(all(test, feature = "logstash-integration-tests"))]
mod integration_tests {
    use std::time::Duration;

    use futures::Stream;
    use tokio::time::timeout;
    use vrl::event_path;

    use super::*;
    use crate::{
        SourceSender,
        config::SourceContext,
        event::EventStatus,
        test_util::{
            collect_n,
            components::{SOCKET_PUSH_SOURCE_TAGS, assert_source_compliance},
            wait_for_tcp,
        },
        tls::{TlsConfig, TlsEnableableConfig},
    };

    fn heartbeat_address() -> String {
        std::env::var("HEARTBEAT_ADDRESS")
            .expect("Address of Beats Heartbeat service must be specified.")
    }

    #[tokio::test]
    async fn beats_heartbeat() {
        let events = assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let out = source(heartbeat_address(), None).await;

            timeout(Duration::from_secs(60), collect_n(out, 1))
                .await
                .unwrap()
        })
        .await;

        assert!(!events.is_empty());

        let log = events[0].as_log();
        assert_eq!(
            log.get(event_path!("@metadata", "beat")),
            Some(String::from("heartbeat").into()).as_ref()
        );
        assert_eq!(
            log.get(event_path!("summary", "up")),
            Some(1.into()).as_ref()
        );
        assert!(log.get(event_path!("timestamp")).is_some());
        assert!(log.get(event_path!("host")).is_some());
    }

    fn logstash_address() -> String {
        std::env::var("LOGSTASH_ADDRESS")
            .expect("Listen address for `logstash` source must be specified.")
    }

    #[tokio::test]
    async fn logstash() {
        let events = assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
            let out = source(
                logstash_address(),
                Some(TlsEnableableConfig {
                    enabled: Some(true),
                    options: TlsConfig {
                        crt_file: Some(
                            "tests/integration/shared/data/host.docker.internal.crt".into(),
                        ),
                        key_file: Some(
                            "tests/integration/shared/data/host.docker.internal.key".into(),
                        ),
                        ..Default::default()
                    },
                }),
            )
            .await;

            timeout(Duration::from_secs(60), collect_n(out, 1))
                .await
                .unwrap()
        })
        .await;

        assert!(!events.is_empty());

        let log = events[0].as_log();
        assert!(
            log.get(event_path!("line"))
                .unwrap()
                .to_string_lossy()
                .contains("Hello World")
        );
        assert!(log.get(event_path!("host")).is_some());
    }

    async fn source(
        address: String,
        tls: Option<TlsEnableableConfig>,
    ) -> impl Stream<Item = Event> + Unpin {
        let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
        let address: SocketAddr = address.parse().unwrap();
        let tls_config = TlsSourceConfig {
            client_metadata_key: None,
            tls_config: tls.unwrap_or_default(),
        };
        tokio::spawn(async move {
            LogstashConfig {
                address: address.into(),
                tls: Some(tls_config),
                keepalive: None,
                permit_origin: None,
                receive_buffer_bytes: None,
                acknowledgements: false.into(),
                connection_limit: None,
                log_namespace: None,
            }
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;
        recv
    }
}
