use std::io::{self, Read};

use bytes::{Buf, Bytes, BytesMut};
use flate2::read::MultiGzDecoder;
use rmp_serde::{decode, Deserializer};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use tokio_util::codec::Decoder;

use super::util::{SocketListenAddr, StreamDecodingError, TcpSource, TcpSourceAck, TcpSourceAcker};
use crate::{
    config::{
        log_schema, AcknowledgementsConfig, DataType, GenerateConfig, Output, Resource,
        SourceConfig, SourceContext, SourceDescription,
    },
    event::{Event, LogEvent},
    internal_events::{FluentMessageDecodeError, FluentMessageReceived},
    serde::bool_or_struct,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
};

mod message;
use self::message::{FluentEntry, FluentMessage, FluentRecord, FluentTag, FluentTimestamp};

#[derive(Deserialize, Serialize, Debug)]
pub struct FluentConfig {
    address: SocketListenAddr,
    tls: Option<TlsConfig>,
    keepalive: Option<TcpKeepaliveConfig>,
    receive_buffer_bytes: Option<usize>,
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
    connection_limit: Option<u32>,
}

inventory::submit! {
    SourceDescription::new::<FluentConfig>("fluent")
}

impl GenerateConfig for FluentConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: SocketListenAddr::SocketAddr("0.0.0.0:24224".parse().unwrap()),
            keepalive: None,
            tls: None,
            receive_buffer_bytes: None,
            acknowledgements: Default::default(),
            connection_limit: Some(2),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "fluent")]
impl SourceConfig for FluentConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let source = FluentSource {};
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

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "fluent"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.into()]
    }
}

#[derive(Debug, Clone)]
struct FluentSource;

impl TcpSource for FluentSource {
    type Error = DecodeError;
    type Item = FluentFrame;
    type Decoder = FluentDecoder;
    type Acker = FluentAcker;

    fn decoder(&self) -> Self::Decoder {
        FluentDecoder::new()
    }

    fn handle_events(&self, events: &mut [Event], host: Bytes, _byte_size: usize) {
        for event in events {
            let log = event.as_mut_log();

            if !log.contains(log_schema().host_key()) {
                log.insert(log_schema().host_key(), host.clone());
            }
        }
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
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::IO(err) => write!(f, "{}", err),
            DecodeError::Decode(err) => write!(f, "{}", err),
            DecodeError::UnknownCompression(compression) => {
                write!(f, "unknown compression: {}", compression)
            }
            DecodeError::UnexpectedValue(value) => {
                write!(f, "unexpected msgpack value, ignoring: {}", value)
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

#[derive(Debug)]
struct FluentDecoder;

impl FluentDecoder {
    const fn new() -> Self {
        FluentDecoder
    }

    fn handle_message(
        &mut self,
        message: Result<FluentMessage, DecodeError>,
        byte_size: usize,
    ) -> Result<Option<(FluentFrame, usize)>, DecodeError> {
        match message? {
            FluentMessage::Message(tag, timestamp, record) => {
                let event = Event::from(FluentEvent {
                    tag,
                    timestamp,
                    record,
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
                    Some("gzip") => {
                        let mut buf = Vec::new();
                        MultiGzDecoder::new(io::Cursor::new(bin.into_vec()))
                            .read_to_end(&mut buf)
                            .map(|_| buf)
                            .map_err(Into::into)
                    }
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
                {
                    if custom.kind() == io::ErrorKind::UnexpectedEof {
                        return Ok(None);
                    }
                }

                (des.position() as usize, res)
            };

            src.advance(byte_size);

            let maybe_item = self.handle_message(res, byte_size).map_err(|error| {
                let base64_encoded_message = base64::encode(&src);
                emit!(&FluentMessageDecodeError {
                    error: &error,
                    base64_encoded_message
                });
                error
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

            if let Err(DecodeError::Decode(decode::Error::InvalidDataRead(ref custom))) = res {
                if custom.kind() == io::ErrorKind::UnexpectedEof {
                    return Ok(None);
                }
            }

            let byte_size = des.position();

            emit!(&FluentMessageReceived { byte_size });

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

        let mut acks = String::new();
        for chunk in self.chunks {
            let ack = match ack {
                TcpSourceAck::Ack => format!(r#"{{"ack": "{}"}}"#, chunk),
                _ => String::from("{}"),
            };
            acks.push_str(&ack);
        }
        Some(acks.into())
    }
}

/// Normalized fluent message.
#[derive(Debug, PartialEq)]
struct FluentEvent {
    tag: FluentTag,
    timestamp: FluentTimestamp,
    record: FluentRecord,
}

impl From<FluentEvent> for Event {
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

impl From<FluentEvent> for LogEvent {
    fn from(frame: FluentEvent) -> LogEvent {
        let FluentEvent {
            tag,
            timestamp,
            record,
        } = frame;

        let mut log = LogEvent::default();
        log.insert(log_schema().timestamp_key(), timestamp);
        log.insert("tag", tag);
        for (key, value) in record.into_iter() {
            log.insert_flat(key, value);
        }
        log
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use chrono::{DateTime, Utc};
    use rmp_serde::Serializer;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        time::{error::Elapsed, timeout, Duration},
    };
    use tokio_util::codec::Decoder;
    use vector_common::{assert_event_data_eq, btreemap};
    use vector_core::event::Value;

    use super::{message::FluentMessageOptions, *};
    use crate::{
        config::{SourceConfig, SourceContext},
        event::EventStatus,
        test_util::{self, next_addr, trace_init, wait_for_tcp},
        SourceSender,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<FluentConfig>();
    }

    // useful references for msgpack:
    // Spec: https://github.com/msgpack/msgpack/blob/master/spec.md
    // Encode to array of bytes: https://kawanet.github.io/msgpack-lite/
    // Decode base64: https://toolslick.com/conversion/data/messagepack-to-json

    #[test]
    fn decode_message_mode() {
        //[
        //  "tag.name",
        //  1441588984,
        //  {"message": "bar"},
        //]
        let message: Vec<u8> = vec![
            147, 168, 116, 97, 103, 46, 110, 97, 109, 101, 206, 85, 236, 230, 248, 129, 167, 109,
            101, 115, 115, 97, 103, 101, 163, 98, 97, 114,
        ];

        let expected = Event::from(btreemap! {
            "message" => "bar",
            "tag" => "tag.name",
            "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
        });
        let got = decode_all(message.clone()).unwrap();
        assert_event_data_eq!(got.0[0], expected);
        assert_eq!(got.1, message.len());
    }

    #[test]
    fn decode_message_mode_with_options() {
        //[
        //  "tag.name",
        //   1441588984,
        //   { "message": "bar" },
        //   { "size": 1 }
        //]
        let message: Vec<u8> = vec![
            148, 168, 116, 97, 103, 46, 110, 97, 109, 101, 206, 85, 236, 230, 248, 129, 167, 109,
            101, 115, 115, 97, 103, 101, 163, 98, 97, 114, 129, 164, 115, 105, 122, 101, 1,
        ];

        let expected = Event::from(btreemap! {
            "message" => "bar",
            "tag" => "tag.name",
            "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
        });
        let got = decode_all(message.clone()).unwrap();
        assert_eq!(got.1, message.len());
        assert_event_data_eq!(got.0[0], expected);
    }

    #[test]
    fn decode_forward_mode() {
        //[
        //    "tag.name",
        //    [
        //        [1441588984, {"message": "foo"}],
        //        [1441588985, {"message": "bar"}],
        //        [1441588986, {"message": "baz"}]
        //    ]
        //]
        let message: Vec<u8> = vec![
            146, 168, 116, 97, 103, 46, 110, 97, 109, 101, 147, 146, 206, 85, 236, 230, 248, 129,
            167, 109, 101, 115, 115, 97, 103, 101, 163, 102, 111, 111, 146, 206, 85, 236, 230, 249,
            129, 167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 114, 146, 206, 85, 236, 230,
            250, 129, 167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 122,
        ];

        let expected = vec![
            Event::from(btreemap! {
                "message" => "foo",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
            }),
            Event::from(btreemap! {
                "message" => "bar",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:05Z").unwrap().into()),
            }),
            Event::from(btreemap! {
                "message" => "baz",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:06Z").unwrap().into()),
            }),
        ];

        let got = decode_all(message.clone()).unwrap();

        assert_eq!(got.1, message.len());
        assert_event_data_eq!(got.0[0], expected[0]);
        assert_event_data_eq!(got.0[1], expected[1]);
        assert_event_data_eq!(got.0[2], expected[2]);
    }

    #[test]
    fn decode_forward_mode_with_options() {
        //[
        //    "tag.name",
        //    [
        //        [1441588984, {"message": "foo"}],
        //        [1441588985, {"message": "bar"}],
        //        [1441588986, {"message": "baz"}]
        //    ],
        //    {"size": 3}
        //]
        let message: Vec<u8> = vec![
            147, 168, 116, 97, 103, 46, 110, 97, 109, 101, 147, 146, 206, 85, 236, 230, 248, 129,
            167, 109, 101, 115, 115, 97, 103, 101, 163, 102, 111, 111, 146, 206, 85, 236, 230, 249,
            129, 167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 114, 146, 206, 85, 236, 230,
            250, 129, 167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 122, 129, 164, 115, 105,
            122, 101, 3,
        ];

        let expected = vec![
            Event::from(btreemap! {
                "message" => "foo",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
            }),
            Event::from(btreemap! {
                "message" => "bar",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:05Z").unwrap().into()),
            }),
            Event::from(btreemap! {
                "message" => "baz",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:06Z").unwrap().into()),
            }),
        ];

        let got = decode_all(message.clone()).unwrap();

        assert_eq!(got.1, message.len());

        assert_event_data_eq!(got.0[0], expected[0]);
        assert_event_data_eq!(got.0[1], expected[1]);
        assert_event_data_eq!(got.0[2], expected[2]);
    }

    #[test]
    fn decode_packed_forward_mode() {
        //[
        //    "tag.name",
        //    <packed messages>
        //]
        //
        //With packed messages as bin:
        // [1441588984, {"message": "foo"}]
        // [1441588985, {"message": "bar"}]
        // [1441588986, {"message": "baz"}]
        let message: Vec<u8> = vec![
            147, 168, 116, 97, 103, 46, 110, 97, 109, 101, 196, 57, 146, 206, 85, 236, 230, 248,
            129, 167, 109, 101, 115, 115, 97, 103, 101, 163, 102, 111, 111, 146, 206, 85, 236, 230,
            249, 129, 167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 114, 146, 206, 85, 236,
            230, 250, 129, 167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 122, 129, 167, 109,
            101, 115, 115, 97, 103, 101, 163, 102, 111, 111,
        ];

        let expected = vec![
            Event::from(btreemap! {
                "message" => "foo",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
            }),
            Event::from(btreemap! {
                "message" => "bar",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:05Z").unwrap().into()),
            }),
            Event::from(btreemap! {
                "message" => "baz",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:06Z").unwrap().into()),
            }),
        ];

        let got = decode_all(message.clone()).unwrap();

        assert_eq!(got.1, message.len());
        assert_event_data_eq!(got.0[0], expected[0]);
        assert_event_data_eq!(got.0[1], expected[1]);
        assert_event_data_eq!(got.0[2], expected[2]);
    }

    //  TODO
    #[test]
    fn decode_compressed_packed_forward_mode() {
        //[
        //    "tag.name",
        //    <packed messages>,
        //    {"compressed": "gzip"}
        //]
        //
        //With gzip'd packed messages as bin:
        // [1441588984, {"message": "foo"}]
        // [1441588985, {"message": "bar"}]
        // [1441588986, {"message": "baz"}]
        let message: Vec<u8> = vec![
            147, 168, 116, 97, 103, 46, 110, 97, 109, 101, 196, 55, 31, 139, 8, 0, 245, 10, 168,
            96, 0, 3, 155, 116, 46, 244, 205, 179, 31, 141, 203, 115, 83, 139, 139, 19, 211, 83,
            23, 167, 229, 231, 79, 2, 9, 253, 68, 8, 37, 37, 22, 129, 133, 126, 33, 11, 85, 1, 0,
            53, 3, 158, 28, 57, 0, 0, 0, 129, 170, 99, 111, 109, 112, 114, 101, 115, 115, 101, 100,
            164, 103, 122, 105, 112,
        ];

        let expected = vec![
            Event::from(btreemap! {
                "message" => "foo",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
            }),
            Event::from(btreemap! {
                "message" => "bar",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:05Z").unwrap().into()),
            }),
            Event::from(btreemap! {
                "message" => "baz",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:06Z").unwrap().into()),
            }),
        ];

        let got = decode_all(message.clone()).unwrap();

        assert_eq!(got.1, message.len());
        assert_event_data_eq!(got.0[0], expected[0]);
        assert_event_data_eq!(got.0[1], expected[1]);
        assert_event_data_eq!(got.0[2], expected[2]);
    }

    fn decode_all(message: Vec<u8>) -> Result<(SmallVec<[Event; 1]>, usize), DecodeError> {
        let mut buf = BytesMut::from(&message[..]);

        let mut decoder = FluentDecoder::new();

        let (frame, byte_size) = decoder.decode(&mut buf)?.unwrap();
        Ok((frame.into(), byte_size))
    }

    #[tokio::test]
    async fn ack_delivered_without_chunk() {
        let (result, output) = check_acknowledgements(EventStatus::Delivered, false).await;
        assert!(matches!(result, Err(_))); // the `_` inside this error is `Elapsed`
        assert!(output.is_empty());
    }

    #[tokio::test]
    async fn ack_delivered_with_chunk() {
        let (result, output) = check_acknowledgements(EventStatus::Delivered, true).await;
        assert_eq!(result.unwrap().unwrap(), output.len());
        assert!(output.starts_with(b"{\"ack\":"));
    }

    #[tokio::test]
    async fn ack_failed_without_chunk() {
        let (result, output) = check_acknowledgements(EventStatus::Rejected, false).await;
        assert_eq!(result.unwrap().unwrap(), output.len());
        assert!(output.is_empty());
    }

    #[tokio::test]
    async fn ack_failed_with_chunk() {
        let (result, output) = check_acknowledgements(EventStatus::Rejected, true).await;
        assert_eq!(result.unwrap().unwrap(), output.len());
        assert_eq!(output, &b"{}"[..]);
    }

    async fn check_acknowledgements(
        status: EventStatus,
        with_chunk: bool,
    ) -> (Result<Result<usize, std::io::Error>, Elapsed>, Bytes) {
        trace_init();

        let (sender, recv) = SourceSender::new_test_finalize(status);
        let address = next_addr();
        let source = FluentConfig {
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

        let msg = uuid::Uuid::new_v4().to_string();
        let tag = uuid::Uuid::new_v4().to_string();
        let req = build_req(&tag, &[("field", &msg)], with_chunk);

        let sender = tokio::spawn(async move {
            let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
            socket.write_all(&req).await.unwrap();

            let mut output = BytesMut::new();
            (
                timeout(Duration::from_millis(250), socket.read_buf(&mut output)).await,
                output,
            )
        });
        let events = test_util::collect_n(recv, 1).await;
        let (result, output) = sender.await.unwrap();

        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert_eq!(log.get("field").unwrap(), &msg.into());
        assert!(matches!(log.get("host").unwrap(), Value::Bytes(_)));
        assert!(matches!(log.get("timestamp").unwrap(), Value::Timestamp(_)));
        assert_eq!(log.get("tag").unwrap(), &tag.into());

        (result, output.into())
    }

    fn build_req(tag: &str, fields: &[(&str, &str)], with_chunk: bool) -> Vec<u8> {
        let mut record = FluentRecord::default();
        for (tag, value) in fields {
            record.insert((*tag).into(), rmpv::Value::String((*value).into()).into());
        }
        let chunk = with_chunk.then(|| base64::encode(uuid::Uuid::new_v4().as_bytes()));
        let req = FluentMessage::MessageWithOptions(
            tag.into(),
            FluentTimestamp::Unix(Utc::now()),
            record,
            FluentMessageOptions {
                chunk,
                ..Default::default()
            },
        );
        let mut buf = Vec::new();
        req.serialize(&mut Serializer::new(&mut buf)).unwrap();
        buf
    }
}

#[cfg(all(test, feature = "fluent-integration-tests"))]
mod integration_tests {
    use std::{fs::File, io::Write, net::SocketAddr, time::Duration};

    use futures::Stream;
    use tokio::time::sleep;
    use vector_core::event::{Event, EventStatus};

    use crate::{
        config::{SourceConfig, SourceContext},
        docker::Container,
        sources::fluent::FluentConfig,
        test_util::{
            collect_ready, next_addr, next_addr_for_ip, random_string, trace_init, wait_for_tcp,
        },
        SourceSender,
    };

    const FLUENT_BIT_IMAGE: &str = "fluent/fluent-bit";
    const FLUENT_BIT_TAG: &str = "1.7";
    const FLUENTD_IMAGE: &str = "fluent/fluentd";
    const FLUENTD_TAG: &str = "v1.12";

    fn make_file(name: &str, content: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mut file = File::create(dir.path().join(name)).unwrap();
        write!(&mut file, "{}", content).unwrap();
        dir
    }

    #[tokio::test]
    async fn fluentbit() {
        test_fluentbit(EventStatus::Delivered).await;
    }

    #[tokio::test]
    async fn fluentbit_rejection() {
        test_fluentbit(EventStatus::Rejected).await;
    }

    async fn test_fluentbit(status: EventStatus) {
        trace_init();

        let test_address = next_addr();
        let (out, source_address) = source(status).await;

        let dir = make_file(
            "fluent-bit.conf",
            &format!(
                r#"
[SERVICE]
    Grace      0
    Flush      1
    Daemon     off

[INPUT]
    Name       http
    Host       {listen_host}
    Port       {listen_port}

[OUTPUT]
    Name          forward
    Match         *
    Host          host.docker.internal
    Port          {send_port}
    Require_ack_response true
"#,
                listen_host = test_address.ip(),
                listen_port = test_address.port(),
                send_port = source_address.port(),
            ),
        );

        let msg = random_string(64);
        let body = serde_json::json!({ "message": msg });

        let events = Container::new(FLUENT_BIT_IMAGE, FLUENT_BIT_TAG)
            .bind(dir.path().display(), "/fluent-bit/etc")
            .run(async move {
                wait_for_tcp(test_address).await;
                reqwest::Client::new()
                    .post(&format!("http://{}/", test_address))
                    .header("content-type", "application/json")
                    .body(body.to_string())
                    .send()
                    .await
                    .unwrap();
                sleep(Duration::from_secs(2)).await;
                let result = collect_ready(out).await;
                result
            })
            .await;

        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert_eq!(log["tag"], "http.0".into());
        assert_eq!(log["message"], msg.into());
        assert!(log.get("timestamp").is_some());
        assert!(log.get("host").is_some());
    }

    #[tokio::test]
    async fn fluentd() {
        test_fluentd(EventStatus::Delivered, "").await;
    }

    #[tokio::test]
    async fn fluentd_gzip() {
        test_fluentd(EventStatus::Delivered, "compress gzip").await;
    }

    #[tokio::test]
    async fn fluentd_rejection() {
        test_fluentd(EventStatus::Rejected, "").await;
    }

    async fn test_fluentd(status: EventStatus, options: &str) {
        trace_init();

        let test_address = next_addr();
        let (out, source_address) = source(status).await;

        let config = format!(
            r#"
<source>
  @type http
  bind {http_host}
  port {http_port}
</source>

<match *>
  @type forward
  <server>
    name  local
    host  host.docker.internal
    port  {port}
  </server>
  <buffer>
    flush_mode immediate
  </buffer>
  require_ack_response true
  ack_response_timeout 1
  {options}
</match>
"#,
            http_host = test_address.ip(),
            http_port = test_address.port(),
            port = source_address.port(),
            options = options
        );

        let dir = make_file("fluent.conf", &config);

        let msg = random_string(64);
        let body = serde_json::json!({ "message": msg });

        let events = Container::new(FLUENTD_IMAGE, FLUENTD_TAG)
            .bind(dir.path().display(), "/fluentd/etc")
            .run(async move {
                wait_for_tcp(test_address).await;
                reqwest::Client::new()
                    .post(&format!("http://{}/", test_address))
                    .header("content-type", "application/json")
                    .body(body.to_string())
                    .send()
                    .await
                    .unwrap();
                sleep(Duration::from_secs(2)).await;
                collect_ready(out).await
            })
            .await;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_log()["tag"], "".into());
        assert_eq!(events[0].as_log()["message"], msg.into());
        assert!(events[0].as_log().get("timestamp").is_some());
        assert!(events[0].as_log().get("host").is_some());
    }

    async fn source(status: EventStatus) -> (impl Stream<Item = Event>, SocketAddr) {
        let (sender, recv) = SourceSender::new_test_finalize(status);
        let address = next_addr_for_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        tokio::spawn(async move {
            FluentConfig {
                address: address.into(),
                tls: None,
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
