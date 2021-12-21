use std::{
    collections::{BTreeMap, VecDeque},
    convert::TryFrom,
    io::{self, Read},
};

use bytes::{Buf, Bytes, BytesMut};
use flate2::read::ZlibDecoder;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Decoder;

use super::util::{SocketListenAddr, StreamDecodingError, TcpSource, TcpSourceAck, TcpSourceAcker};
use crate::{
    config::{
        log_schema, AcknowledgementsConfig, DataType, GenerateConfig, Resource, SourceConfig,
        SourceContext, SourceDescription,
    },
    event::{Event, Value},
    serde::bool_or_struct,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
    types,
};

#[derive(Deserialize, Serialize, Debug)]
pub struct LogstashConfig {
    address: SocketListenAddr,
    keepalive: Option<TcpKeepaliveConfig>,
    tls: Option<TlsConfig>,
    receive_buffer_bytes: Option<usize>,
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
    connection_limit: Option<u32>,
}

inventory::submit! {
    SourceDescription::new::<LogstashConfig>("logstash")
}

impl GenerateConfig for LogstashConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: SocketListenAddr::SocketAddr("0.0.0.0:5044".parse().unwrap()),
            keepalive: None,
            tls: None,
            receive_buffer_bytes: None,
            acknowledgements: Default::default(),
            connection_limit: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "logstash")]
impl SourceConfig for LogstashConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let source = LogstashSource {
            timestamp_converter: types::Conversion::Timestamp(cx.globals.timezone),
        };
        let shutdown_secs = 30;
        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        source.run(
            self.address,
            self.keepalive,
            shutdown_secs,
            tls,
            self.receive_buffer_bytes,
            cx,
            self.acknowledgements,
            self.connection_limit,
        )
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "logstash"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.into()]
    }
}

#[derive(Debug, Clone)]
struct LogstashSource {
    timestamp_converter: crate::types::Conversion,
}

impl TcpSource for LogstashSource {
    type Error = DecodeError;
    type Item = LogstashEventFrame;
    type Decoder = LogstashDecoder;
    type Acker = LogstashAcker;

    fn decoder(&self) -> Self::Decoder {
        LogstashDecoder::new()
    }

    fn handle_events(&self, events: &mut [Event], host: Bytes, _byte_size: usize) {
        let now = Value::from(chrono::Utc::now());
        for event in events {
            let log = event.as_mut_log();
            log.try_insert(log_schema().source_type_key(), "logstash");
            if log.get(log_schema().timestamp_key()).is_none() {
                // Attempt to parse @timestamp if it exists; otherwise set to receipt time.
                let timestamp = log
                    .get_flat("@timestamp")
                    .and_then(|timestamp| {
                        self.timestamp_converter
                            .convert::<Value>(timestamp.as_bytes())
                            .ok()
                    })
                    .unwrap_or_else(|| now.clone());
                log.insert(log_schema().timestamp_key(), timestamp);
            }
            log.try_insert(log_schema().host_key(), host.clone());
        }
    }

    fn build_acker(&self, frames: &[Self::Item]) -> Self::Acker {
        LogstashAcker::new(frames)
    }
}

struct LogstashAcker {
    sequence_number: u32,
    protocol_version: Option<LogstashProtocolVersion>,
}

impl LogstashAcker {
    fn new(frames: &[LogstashEventFrame]) -> Self {
        let mut sequence_number = 0;
        let mut protocol_version = None;

        for frame in frames {
            sequence_number = std::cmp::max(sequence_number, frame.sequence_number);
            // We assume that it's valid to ack via any of the protocol versions that we've seen in
            // a set of frames from a single stream, so here we just take the last. In reality, we
            // do not expect stream with multiple protocol versions to occur.
            protocol_version = Some(frame.protocol);
        }

        Self {
            sequence_number,
            protocol_version,
        }
    }
}

impl TcpSourceAcker for LogstashAcker {
    // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#ack-frame-type
    fn build_ack(self, ack: TcpSourceAck) -> Option<Bytes> {
        match (ack, self.protocol_version) {
            (TcpSourceAck::Ack, Some(protocol_version)) => {
                let mut bytes: Vec<u8> = Vec::with_capacity(6);
                bytes.push(protocol_version.into());
                bytes.push(LogstashFrameType::Ack.into());
                bytes.extend(self.sequence_number.to_be_bytes().iter());
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
}

impl LogstashDecoder {
    const fn new() -> Self {
        Self {
            state: LogstashDecoderReadState::ReadProtocol,
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
    fields: BTreeMap<String, serde_json::Value>,
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
                        V1 => LogstashDecoderReadState::ReadType(LogstashProtocolVersion::V1),
                        V2 => LogstashDecoderReadState::ReadType(LogstashProtocolVersion::V2),
                    }
                }
                LogstashDecoderReadState::ReadType(protocol) => {
                    if src.remaining() < 1 {
                        return Ok(None);
                    }

                    use LogstashFrameType::*;

                    match LogstashFrameType::try_from(src.get_u8())? {
                        WindowSize => LogstashDecoderReadState::ReadFrame(
                            protocol,
                            LogstashFrameType::WindowSize,
                        ),
                        Data => {
                            LogstashDecoderReadState::ReadFrame(protocol, LogstashFrameType::Data)
                        }
                        Json => {
                            LogstashDecoderReadState::ReadFrame(protocol, LogstashFrameType::Json)
                        }
                        Compressed => LogstashDecoderReadState::ReadFrame(
                            protocol,
                            LogstashFrameType::Compressed,
                        ),
                        Ack => {
                            LogstashDecoderReadState::ReadFrame(protocol, LogstashFrameType::Ack)
                        }
                    }
                }
                // The window size indicates how many events the writer will send before waiting
                // for acks. As we forward events as we get them, and ack as they are received, we
                // do not need to keep track of this.
                //
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#window-size-frame-type
                LogstashDecoderReadState::ReadFrame(_protocol, LogstashFrameType::WindowSize) => {
                    if src.remaining() < 4 {
                        return Ok(None);
                    }

                    let _window_size = src.get_u32();

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
                    let mut rest = src.as_ref();

                    if rest.remaining() < 8 {
                        return Ok(None);
                    }
                    let sequence_number = rest.get_u32();
                    let pair_count = rest.get_u32();

                    let mut fields: BTreeMap<String, serde_json::Value> = BTreeMap::new();
                    for _ in 0..pair_count {
                        if src.remaining() < 4 {
                            return Ok(None);
                        }
                        let key_length = rest.get_u32() as usize;

                        if rest.remaining() < key_length {
                            return Ok(None);
                        }
                        let (key, right) = rest.split_at(key_length);
                        rest = right;

                        if src.remaining() < 4 {
                            return Ok(None);
                        }
                        let value_length = rest.get_u32() as usize;
                        if rest.remaining() < value_length {
                            return Ok(None);
                        }
                        let (value, right) = rest.split_at(value_length);
                        rest = right;

                        fields.insert(
                            String::from_utf8_lossy(key).to_string(),
                            String::from_utf8_lossy(value).into(),
                        );
                    }

                    let remaining = rest.remaining();
                    let byte_size = src.remaining() - remaining;

                    src.advance(byte_size);

                    let frames = vec![(
                        LogstashEventFrame {
                            protocol,
                            sequence_number,
                            fields,
                        },
                        byte_size,
                    )]
                    .into();

                    LogstashDecoderReadState::PendingFrames(frames)
                }
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#json-frame-type
                LogstashDecoderReadState::ReadFrame(protocol, LogstashFrameType::Json) => {
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

                    let fields_result: Result<BTreeMap<String, serde_json::Value>, _> =
                        serde_json::from_slice(slice).context(JsonFrameFailedDecode {});

                    let remaining = rest.remaining();
                    let byte_size = src.remaining() - remaining;

                    src.advance(byte_size);

                    match fields_result {
                        Ok(fields) => {
                            let frames = vec![(
                                LogstashEventFrame {
                                    protocol,
                                    sequence_number,
                                    fields,
                                },
                                byte_size,
                            )]
                            .into();

                            LogstashDecoderReadState::PendingFrames(frames)
                        }
                        Err(err) => return Err(err),
                    }
                }
                // https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md#compressed-frame-type
                LogstashDecoderReadState::ReadFrame(_protocol, LogstashFrameType::Compressed) => {
                    let mut rest = src.as_ref();

                    if rest.remaining() < 4 {
                        return Ok(None);
                    }
                    let payload_size = rest.get_u32() as usize;

                    if rest.remaining() < payload_size {
                        src.reserve(payload_size as usize);
                        return Ok(None);
                    }

                    let (slice, right) = rest.split_at(payload_size);
                    rest = right;

                    let mut buf = {
                        let mut buf = Vec::new();

                        let res = ZlibDecoder::new(io::Cursor::new(slice))
                            .read_to_end(&mut buf)
                            .context(DecompressionFailed)
                            .map(|_| BytesMut::from(&buf[..]));

                        let remaining = rest.remaining();
                        let byte_size = src.remaining() - remaining;

                        src.advance(byte_size);

                        res
                    }?;

                    let mut decoder = LogstashDecoder::new();

                    let mut frames = VecDeque::new();

                    while let Some(s) = decoder.decode(&mut buf)? {
                        frames.push_back(s);
                    }

                    LogstashDecoderReadState::PendingFrames(frames)
                }
            };
        }
    }
}

impl From<LogstashEventFrame> for Event {
    fn from(frame: LogstashEventFrame) -> Self {
        frame
            .fields
            .into_iter()
            .map(|(key, value)| (key, Value::from(value)))
            .collect::<BTreeMap<_, _>>()
            .into()
    }
}

impl From<LogstashEventFrame> for SmallVec<[Event; 1]> {
    fn from(frame: LogstashEventFrame) -> Self {
        smallvec![frame.into()]
    }
}

#[cfg(test)]
mod test {
    use bytes::BufMut;
    use rand::{thread_rng, Rng};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::{
        event::EventStatus,
        test_util::{next_addr, spawn_collect_n, wait_for_tcp},
        Pipeline,
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

    async fn test_protocol(status: EventStatus, sends_ack: bool) {
        let (sender, recv) = Pipeline::new_test_finalize(status);
        let address = next_addr();
        let source = LogstashConfig {
            address: address.into(),
            tls: None,
            keepalive: None,
            receive_buffer_bytes: None,
            acknowledgements: true.into(),
            connection_limit: None,
        }
        .build(SourceContext::new_test(sender))
        .await
        .unwrap();
        tokio::spawn(source);
        wait_for_tcp(address).await;

        let events = spawn_collect_n(
            send_req(address, &[("message", "Hello, world!")], sends_ack),
            recv,
            1,
        )
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

    fn encode_req(seq: u32, pairs: &[(&str, &str)]) -> Bytes {
        let mut req = BytesMut::new();
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
        req.into()
    }

    async fn send_req(address: std::net::SocketAddr, pairs: &[(&str, &str)], sends_ack: bool) {
        let seq = thread_rng().gen_range(1..u32::MAX);
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
}

#[cfg(all(test, feature = "logstash-integration-tests"))]
mod integration_tests {
    use std::{fs::File, io::Write, net::SocketAddr, time::Duration};

    use futures::Stream;
    use tokio::time::timeout;

    use super::*;
    use crate::{
        config::SourceContext,
        docker::Container,
        event::EventStatus,
        test_util::{collect_n, next_addr_for_ip, trace_init, wait_for_tcp},
        tls::TlsOptions,
        Pipeline,
    };

    const BEATS_IMAGE: &str = "docker.elastic.co/beats/heartbeat";
    const BEATS_TAG: &str = "7.12.1";

    const LOGSTASH_IMAGE: &str = "docker.elastic.co/logstash/logstash";
    const LOGSTASH_TAG: &str = "7.13.1";

    #[tokio::test]
    async fn beats_heartbeat() {
        trace_init();

        let (out, address) = source(None).await;

        let dir = tempfile::tempdir().unwrap();
        let mut file = File::create(dir.path().join("heartbeat.yml")).unwrap();
        write!(
            &mut file,
            r#"
heartbeat.monitors:
- type: http
  schedule: '@every 1s'
  urls:
    - https://google.com

output.logstash:
  hosts: ['host.docker.internal:{}']
"#,
            address.port()
        )
        .unwrap();

        let events = Container::new(BEATS_IMAGE, BEATS_TAG)
            .bind(
                dir.path().join("heartbeat.yml").display(),
                "/usr/share/heartbeat/heartbeat.yml",
            )
            // adding `-strict.perms=false to the default cmd as otherwise heartbeat was
            // complaining about the file permissions when running in CI
            // https://www.elastic.co/guide/en/beats/libbeat/5.3/config-file-permissions.html
            .cmd("-environment=container")
            .cmd("-strict.perms=false")
            .run(timeout(Duration::from_secs(60), collect_n(out, 1)))
            .await
            .unwrap();

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

    #[tokio::test]
    async fn logstash() {
        trace_init();

        let (out, address) = source(Some(TlsConfig {
            enabled: Some(true),
            options: TlsOptions {
                crt_file: Some("tests/data/host.docker.internal.crt".into()),
                key_file: Some("tests/data/host.docker.internal.key".into()),
                ..Default::default()
            },
        }))
        .await;

        let dir = tempfile::tempdir().unwrap();
        let mut file = File::create(dir.path().join("logstash.conf")).unwrap();
        write!(
            &mut file,
            "{}",
            r#"
input {
  generator {
    count => 5
    message => "Hello World"
  }
}
output {
  lumberjack {
    hosts => "host.docker.internal"
    ssl_certificate => "/tmp/logstash.crt"
    port => PORT
  }
}
"#
            .replace("PORT", &address.port().to_string())
        )
        .unwrap();

        let pwd = std::env::current_dir().unwrap();
        let events = Container::new(LOGSTASH_IMAGE, LOGSTASH_TAG)
            .bind("/dev/null", "/usr/share/logstash/config/logstash.yml") // tries to contact elasticsearch by default
            .bind(
                dir.path().join("logstash.conf").display(),
                "/usr/share/logstash/pipeline/logstash.conf",
            )
            .bind(
                pwd.join("tests/data/host.docker.internal.crt").display(),
                "/tmp/logstash.crt",
            )
            .run(timeout(Duration::from_secs(60), collect_n(out, 1)))
            .await
            .unwrap();

        assert!(!events.is_empty());

        let log = events[0].as_log();
        assert!(log
            .get("line")
            .unwrap()
            .to_string_lossy()
            .contains("Hello World"));
        assert!(log.get("host").is_some());
    }

    async fn source(tls: Option<TlsConfig>) -> (impl Stream<Item = Event>, SocketAddr) {
        let (sender, recv) = Pipeline::new_test_finalize(EventStatus::Delivered);
        let address = next_addr_for_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        tokio::spawn(async move {
            LogstashConfig {
                address: address.into(),
                tls,
                keepalive: None,
                receive_buffer_bytes: None,
                acknowledgements: false.into(),
                connection_limit: None,
            }
            .build(SourceContext::new_test(sender))
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;
        (recv, address)
    }
}
