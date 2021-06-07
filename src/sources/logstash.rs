use super::util::{SocketListenAddr, TcpIsErrorFatal, TcpSource};
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Resource, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::{Event, LogEvent, Value},
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
};
use bytes::{Buf, Bytes, BytesMut};
use flate2::read::ZlibDecoder;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    collections::{BTreeMap, VecDeque},
    convert::TryInto,
    io::{self, Read},
};
use tokio_util::codec::Decoder;

// TODO
// * Handle window size and acking
// * Handle protocol version differences
// * Handle Data frames
// * usize casts bounds
// * Integration tests

#[derive(Deserialize, Serialize, Debug)]
pub struct LogstashConfig {
    address: SocketListenAddr,
    keepalive: Option<TcpKeepaliveConfig>,
    tls: Option<TlsConfig>,
    receive_buffer_bytes: Option<usize>,
}

inventory::submit! {
    SourceDescription::new::<LogstashConfig>("logstash")
}

impl GenerateConfig for LogstashConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: SocketListenAddr::SocketAddr("0.0.0.0:514".parse().unwrap()),
            keepalive: None,
            tls: None,
            receive_buffer_bytes: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "logstash")]
impl SourceConfig for LogstashConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let source = LogstashSource {};
        let shutdown_secs = 30;
        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        source.run(
            self.address,
            self.keepalive,
            shutdown_secs,
            tls,
            self.receive_buffer_bytes,
            cx.shutdown,
            cx.out,
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
struct LogstashSource {}

impl TcpSource for LogstashSource {
    type Error = DecodeError;
    type Decoder = LogstashDecoder;

    fn decoder(&self) -> Self::Decoder {
        LogstashDecoder::new()
    }

    fn build_event(
        &self,
        frame: BTreeMap<String, serde_json::Value>,
        host: Bytes,
    ) -> Option<Event> {
        let mut log = LogEvent::from(
            frame
                .into_iter()
                .map(|(key, value)| (key, Value::from(value)))
                .collect::<BTreeMap<_, _>>(),
        );
        if log.get(log_schema().host_key()).is_none() {
            log.insert(log_schema().host_key(), host);
        }
        Some(Event::from(log))
    }
}

#[derive(Debug, Clone)]
enum LogstashDecoderReadState {
    ReadProtocol,
    ReadType(LogstashProtocolVersion),
    ReadFrame(LogstashProtocolVersion, LogstashFrameType),
    PendingFrames(VecDeque<BTreeMap<String, serde_json::Value>>),
}

#[derive(Clone, Debug)]
struct LogstashDecoder {
    state: LogstashDecoderReadState,
}

impl LogstashDecoder {
    fn new() -> Self {
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
    #[snafu(display("Unknown logstash protocol message type: {}", message_type))]
    UnknownMessageType { message_type: char },
    #[snafu(display("Failed to decode JSON frame: {}", source))]
    JsonFrameFailedDecode { source: serde_json::Error },
    #[snafu(display("Failed to decompress compressed frame: {}", source))]
    DecompressionFailed { source: io::Error },
}

impl TcpIsErrorFatal for DecodeError {
    fn is_error_fatal() -> bool {
        // TODO
        // Protocol and message type errors should be unrecoverable since we don't know how much to advance
        // DecompressionFailed, JsonFrameFailedDecode should be false as we can advance past that frame
        // Other i/o errors should be true
        true
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

#[derive(Debug, Clone, Copy)]
enum LogstashFrameType {
    WindowSize, // W
    Data,       // D
    Json,       // J
    Compressed, // C
}

// Based on spec at: https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md
// And implementation from logstash: https://github.com/logstash-plugins/logstash-input-beats/blob/27bad62a26a81fc000a9d21495b8dc7174ab63e9/src/main/java/org/logstash/beats/BeatsParser.java
impl Decoder for LogstashDecoder {
    type Item = BTreeMap<String, serde_json::Value>;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.state {
                LogstashDecoderReadState::PendingFrames(ref mut frames) => {
                    match frames.pop_front() {
                        Some(frame) => return Ok(Some(frame)),
                        None => {
                            self.state = LogstashDecoderReadState::ReadProtocol;
                        }
                    }
                }
                LogstashDecoderReadState::ReadProtocol => {
                    if src.remaining() < 1 {
                        return Ok(None);
                    }
                    match src.get_u8() {
                        b'1' => {
                            self.state =
                                LogstashDecoderReadState::ReadType(LogstashProtocolVersion::V1);
                        }
                        b'2' => {
                            self.state =
                                LogstashDecoderReadState::ReadType(LogstashProtocolVersion::V2);
                        }
                        version => {
                            return Err(DecodeError::UnknownProtocolVersion {
                                version: version as char,
                            });
                        }
                    }
                }
                LogstashDecoderReadState::ReadType(version) => {
                    if src.remaining() < 1 {
                        return Ok(None);
                    }

                    match src.get_u8() {
                        b'W' => {
                            self.state = LogstashDecoderReadState::ReadFrame(
                                version,
                                LogstashFrameType::WindowSize,
                            );
                        }
                        b'D' => {
                            self.state = LogstashDecoderReadState::ReadFrame(
                                version,
                                LogstashFrameType::Data,
                            )
                        }
                        b'J' => {
                            self.state = LogstashDecoderReadState::ReadFrame(
                                version,
                                LogstashFrameType::Json,
                            )
                        }
                        b'C' => {
                            self.state = LogstashDecoderReadState::ReadFrame(
                                version,
                                LogstashFrameType::Compressed,
                            )
                        }
                        message_type => {
                            return Err(DecodeError::UnknownMessageType {
                                message_type: message_type as char,
                            });
                        }
                    }
                }
                LogstashDecoderReadState::ReadFrame(_version, LogstashFrameType::WindowSize) => {
                    if src.remaining() < 4 {
                        return Ok(None);
                    }

                    let _window_size = src.get_u32();
                    self.state = LogstashDecoderReadState::ReadProtocol;
                }
                LogstashDecoderReadState::ReadFrame(_version, LogstashFrameType::Data) => {
                    unimplemented!("TODO")
                }
                LogstashDecoderReadState::ReadFrame(_version, LogstashFrameType::Json) => {
                    match (src.get(0..4), src.get(4..8)) {
                        (None, _) | (_, None) => return Ok(None),
                        (Some(_sequence_number), Some(payload_size)) => {
                            let payload_size = u32::from_be_bytes(
                                payload_size.try_into().expect("exactly 4 bytes"),
                            );
                            match &src.get(8..8 + payload_size as usize) {
                                None => {
                                    src.reserve(8 + payload_size as usize);
                                    return Ok(None);
                                }

                                Some(slice) => {
                                    let fields_result: Result<
                                        BTreeMap<String, serde_json::Value>,
                                        _,
                                    > = serde_json::from_slice(&slice[..])
                                        .context(JsonFrameFailedDecode {});

                                    src.advance(8 + payload_size as usize);

                                    self.state = LogstashDecoderReadState::ReadProtocol;

                                    return fields_result.map(Option::Some);
                                }
                            }
                        }
                    }
                }
                LogstashDecoderReadState::ReadFrame(_version, LogstashFrameType::Compressed) => {
                    match src.get(0..4) {
                        None => return Ok(None),
                        Some(bytes) => {
                            let payload_size =
                                u32::from_be_bytes(bytes.try_into().expect("exactly 4 bytes"));

                            match &src.get(4..4 + payload_size as usize) {
                                None => {
                                    src.reserve(4 + payload_size as usize);
                                    return Ok(None);
                                }
                                Some(slice) => {
                                    let mut buf = {
                                        let mut buf = Vec::new();

                                        let res = ZlibDecoder::new(io::Cursor::new(&slice[..]))
                                            .read_to_end(&mut buf)
                                            .context(DecompressionFailed)
                                            .map(|_| BytesMut::from(&buf[..]));

                                        src.advance(4 + payload_size as usize);

                                        res
                                    }?;

                                    let mut decoder = LogstashDecoder::new();

                                    let mut frames = VecDeque::new();

                                    while let Some(s) = decoder.decode(&mut buf)? {
                                        frames.push_back(s);
                                    }

                                    self.state = LogstashDecoderReadState::PendingFrames(frames);
                                }
                            }
                        }
                    }
                }
            };
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<LogstashConfig>();
    }
}
