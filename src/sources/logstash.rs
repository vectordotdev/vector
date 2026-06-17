use std::{
    collections::{BTreeMap, VecDeque},
    convert::TryFrom,
    io::{self, Read},
    net::SocketAddr,
    num::NonZeroUsize,
    time::Duration,
};

use bytes::{Buf, Bytes, BytesMut};
use flate2::read::ZlibDecoder;
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
use vrl::value::{KeyString, Kind, ObjectMap, kind::Collection};

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
    // Batched reads can contain multiple writer windows. Preserve a separate
    // ACK point for each completed window so Filebeat never sees an ACK that
    // advances past the current window it is waiting on. If the batch ends in
    // the middle of a window, ACK the last received event in that final ACK
    // domain so clients are not forced to wait for the advertised window size.
    // Lumberjack defines WindowSize as a maximum unacked count, so a sender can
    // legitimately advertise a fresh window after a previously ACKed partial
    // tail. Within a single ReadyFrames batch, the only incomplete ACK domain
    // we can represent independently is the final tail we have actually seen.
    // We expect most batches to need only one ACK point, either for a single
    // completed window or for one partial tail. Multiple ACKs are only needed
    // when ReadyFrames coalesces multiple logical windows into one batch.
    acknowledgements: SmallVec<[(LogstashProtocolVersion, u32); 1]>,
}

impl LogstashAcker {
    fn new(frames: &[LogstashEventFrame]) -> Self {
        let acknowledgements = frames
            .iter()
            .enumerate()
            // ACK each completed writer window and the last frame in a partial batch if ReadyFrames
            // flushes before the current window is complete.
            .filter(|(index, frame)| frame.window_end || index + 1 == frames.len())
            .map(|(_, frame)| (frame.protocol, frame.sequence_number))
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
}

impl StreamDecodingError for DecodeError {
    fn can_continue(&self) -> bool {
        use DecodeError::*;

        match self {
            IO { .. } => false,
            UnknownProtocolVersion { .. } => false,
            UnknownFrameType { .. } => false,
            JsonFrameFailedDecode { .. } => true,
            DecompressionFailed { .. } => true,
        }
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
                // The window size indicates how many events the writer will send before waiting
                // for acks. We preserve this boundary so the acker can emit one ACK per
                // completed window, even if multiple windows are batched together later.
                // Filebeat accepts cumulative ACKs, but not ACKs that advance past the
                // current writer window it is waiting on. WindowSize is a maximum unacked
                // count, not necessarily an exact count of immediately following frames, so a
                // sender can legitimately advertise a new window after a previously ACKed
                // partial tail. If a malformed sender does this before that earlier tail has
                // actually been ACKed, we tolerate the reset here even though it can collapse
                // the older incomplete domain into the new one.
                //
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#window-size-frame-type
                LogstashDecoderReadState::ReadFrame(_protocol, LogstashFrameType::WindowSize) => {
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

    if rest.remaining() < payload_size {
        src.reserve(payload_size);
        return Ok(None);
    }

    let (slice, right) = rest.split_at(payload_size);
    rest = right;

    let mut buf = Vec::new();

    let res = ZlibDecoder::new(io::Cursor::new(slice))
        .read_to_end(&mut buf)
        .context(DecompressionFailedSnafu)
        .map(|_| BytesMut::from(&buf[..]));

    let byte_size = bytes_remaining(src, rest);
    src.advance(byte_size);

    let mut buf = res?;

    let mut decoder = LogstashDecoder::new_with_window_events_remaining(window_events_remaining);

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
                .collect::<ObjectMap>(),
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
            log.get("message").unwrap().to_string_lossy(),
            "Hello, world!".to_string()
        );
        assert_eq!(
            log.get("source_type").unwrap().to_string_lossy(),
            "logstash".to_string()
        );
        assert!(log.get("host").is_some());
        assert!(log.get("timestamp").is_some());
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

        let ack = LogstashAcker::new(&frames)
            .build_ack(TcpSourceAck::Ack)
            .unwrap();
        let acknowledgements = decode_acknowledgements(ack);

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
    async fn incomplete_final_window_is_acked_to_the_last_received_event() {
        let mut req = BytesMut::new();
        push_window_size(&mut req, 4);
        push_req(&mut req, 1, &[("message", "only event in partial window")]);

        let decoded = decode_frames_and_assert_sequences(req, &[1]);
        assert_acknowledgements_for_ready_frames(decoded, &[1], &[1]).await;
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
    async fn single_window_split_across_ready_frames_keeps_progressive_acks() {
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
            let ack = LogstashAcker::new(&frames)
                .build_ack(TcpSourceAck::Ack)
                .unwrap();
            acknowledgements.push(decode_acknowledgements(ack));
        }

        assert_eq!(acknowledgements, vec![vec![2], vec![4]]);
    }

    #[tokio::test]
    async fn fresh_window_after_acked_partial_tail_is_accepted() {
        let mut decoder = LogstashDecoder::new();

        let mut first_batch = BytesMut::new();
        push_window_size(&mut first_batch, 2);
        push_req(&mut first_batch, 1, &[("message", "first partial tail")]);
        let decoded =
            decode_frames_with_decoder_and_assert_sequences(&mut decoder, first_batch, &[1]);
        assert_acknowledgements_for_ready_frames(decoded, &[1], &[1]).await;

        let mut second_batch = BytesMut::new();
        push_window_size(&mut second_batch, 1);
        push_req(
            &mut second_batch,
            1,
            &[("message", "fresh window after ack")],
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
            log.get("@metadata.beat"),
            Some(String::from("heartbeat").into()).as_ref()
        );
        assert_eq!(log.get("summary.up"), Some(1.into()).as_ref());
        assert!(log.get("timestamp").is_some());
        assert!(log.get("host").is_some());
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
            log.get("line")
                .unwrap()
                .to_string_lossy()
                .contains("Hello World")
        );
        assert!(log.get("host").is_some());
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
