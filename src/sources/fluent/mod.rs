use super::util::{SocketListenAddr, TcpError, TcpSource};
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Resource, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::{Event, LogEvent},
    internal_events::{FluentMessageDecodeError, FluentMessageReceived},
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
};
use bytes::{Buf, Bytes, BytesMut};
use flate2::read::MultiGzDecoder;
use rmp_serde::{decode, Deserializer};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::{
    collections::VecDeque,
    io::{self, Read},
};
use tokio_util::codec::Decoder;

use crate::sources::fluent::message::{
    FluentEntry, FluentMessage, FluentRecord, FluentTag, FluentTimestamp,
};
mod message;

#[derive(Deserialize, Serialize, Debug)]
pub struct FluentConfig {
    address: SocketListenAddr,
    tls: Option<TlsConfig>,
    keepalive: Option<TcpKeepaliveConfig>,
    receive_buffer_bytes: Option<usize>,
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
            cx.shutdown,
            cx.out,
        )
    }

    fn output_type(&self) -> DataType {
        DataType::Log
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

impl TcpError for DecodeError {
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
struct FluentDecoder {
    // unread frames from previous fluent message
    unread_frames: VecDeque<(FluentFrame, usize)>,
}

impl FluentDecoder {
    fn new() -> Self {
        FluentDecoder {
            unread_frames: VecDeque::new(),
        }
    }

    fn handle_message(
        &mut self,
        message: FluentMessage,
        byte_size: usize,
    ) -> Result<(), DecodeError> {
        match message {
            FluentMessage::Message(tag, timestamp, record)
            | FluentMessage::MessageWithOptions(tag, timestamp, record, ..) => {
                self.unread_frames.push_back((
                    FluentFrame {
                        tag,
                        timestamp,
                        record,
                    },
                    byte_size,
                ));
                Ok(())
            }
            FluentMessage::Forward(tag, entries)
            | FluentMessage::ForwardWithOptions(tag, entries, ..) => {
                self.unread_frames.extend(entries.into_iter().map(
                    |FluentEntry(timestamp, record)| {
                        (
                            FluentFrame {
                                tag: tag.clone(),
                                timestamp,
                                record,
                            },
                            byte_size,
                        )
                    },
                ));
                Ok(())
            }
            FluentMessage::PackedForward(tag, bin) => {
                let mut buf = BytesMut::from(&bin[..]);

                let mut decoder = FluentEntryStreamDecoder;

                while let Some(FluentEntry(timestamp, record)) = decoder.decode(&mut buf)? {
                    self.unread_frames.push_back((
                        FluentFrame {
                            tag: tag.clone(),
                            timestamp,
                            record,
                        },
                        byte_size,
                    ));
                }
                Ok(())
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

                let mut decoder = FluentEntryStreamDecoder;

                while let Some(FluentEntry(timestamp, record)) = decoder.decode(&mut buf)? {
                    self.unread_frames.push_back((
                        FluentFrame {
                            tag: tag.clone(),
                            timestamp,
                            record,
                        },
                        byte_size,
                    ));
                }
                Ok(())
            }
            FluentMessage::Heartbeat(rmpv::Value::Nil) => Ok(()),
            FluentMessage::Heartbeat(value) => Err(DecodeError::UnexpectedValue(value)),
        }
    }
}

impl Decoder for FluentDecoder {
    type Item = (FluentFrame, usize);
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(item) = self.unread_frames.pop_front() {
            return Ok(Some(item));
        }

        if src.is_empty() {
            return Ok(None);
        }

        let (byte_size, res) = {
            let mut des = Deserializer::new(io::Cursor::new(&src[..]));

            let res = Deserialize::deserialize(&mut des).map_err(DecodeError::Decode);

            // check for unexpected EOF to indicate that we need more data
            match res {
                // can use or-patterns in 1.53
                // https://github.com/rust-lang/rust/pull/79278
                Err(DecodeError::Decode(decode::Error::InvalidDataRead(ref custom))) => {
                    if custom.kind() == io::ErrorKind::UnexpectedEof {
                        return Ok(None);
                    }
                }
                Err(DecodeError::Decode(decode::Error::InvalidMarkerRead(ref custom))) => {
                    if custom.kind() == io::ErrorKind::UnexpectedEof {
                        return Ok(None);
                    }
                }
                _ => {}
            }

            (des.position() as usize, res)
        };

        src.advance(byte_size);

        res.and_then(|message| {
            self.handle_message(message, byte_size)
                .map(|_| self.unread_frames.pop_front())
        })
        .map_err(|error| {
            let base64_encoded_message = base64::encode(&src);
            emit!(&FluentMessageDecodeError {
                error: &error,
                base64_encoded_message
            });
            error
        })
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

/// Normalized fluent message.
#[derive(Debug, PartialEq)]
struct FluentFrame {
    tag: FluentTag,
    timestamp: FluentTimestamp,
    record: FluentRecord,
}

impl From<FluentFrame> for Event {
    fn from(frame: FluentFrame) -> Event {
        LogEvent::from(frame).into()
    }
}

impl From<FluentFrame> for SmallVec<[Event; 1]> {
    fn from(frame: FluentFrame) -> Self {
        smallvec![frame.into()]
    }
}

impl From<FluentFrame> for LogEvent {
    fn from(frame: FluentFrame) -> LogEvent {
        let FluentFrame {
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
    use crate::sources::fluent::{DecodeError, FluentConfig, FluentDecoder};
    use bytes::BytesMut;
    use chrono::DateTime;
    use shared::{assert_event_data_eq, btreemap};
    use tokio_util::codec::Decoder;
    use vector_core::event::{LogEvent, Value};

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

        let expected = (
            LogEvent::from(btreemap! {
                "message" => "bar",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
            }),
            28,
        );
        let got = decode_all(message).unwrap();
        assert_event_data_eq!(got[0].0, expected.0);
        assert_eq!(got[0].1, expected.1);
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

        let expected = (
            LogEvent::from(btreemap! {
                "message" => "bar",
                "tag" => "tag.name",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
            }),
            35,
        );
        let got = decode_all(message).unwrap();
        assert_event_data_eq!(got[0].0, expected.0);
        assert_eq!(got[0].1, expected.1);
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
            (
                LogEvent::from(btreemap! {
                    "message" => "foo",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
                }),
                68,
            ),
            (
                LogEvent::from(btreemap! {
                    "message" => "bar",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:05Z").unwrap().into()),
                }),
                68,
            ),
            (
                LogEvent::from(btreemap! {
                    "message" => "baz",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:06Z").unwrap().into()),
                }),
                68,
            ),
        ];

        let got = decode_all(message).unwrap();

        assert_event_data_eq!(got[0].0, expected[0].0);
        assert_eq!(got[0].1, expected[0].1);
        assert_event_data_eq!(got[1].0, expected[1].0);
        assert_eq!(got[1].1, expected[1].1);
        assert_event_data_eq!(got[2].0, expected[2].0);
        assert_eq!(got[2].1, expected[2].1);
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
            (
                LogEvent::from(btreemap! {
                    "message" => "foo",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
                }),
                75,
            ),
            (
                LogEvent::from(btreemap! {
                    "message" => "bar",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:05Z").unwrap().into()),
                }),
                75,
            ),
            (
                LogEvent::from(btreemap! {
                    "message" => "baz",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:06Z").unwrap().into()),
                }),
                75,
            ),
        ];

        let got = decode_all(message).unwrap();

        assert_event_data_eq!(got[0].0, expected[0].0);
        assert_eq!(got[0].1, expected[0].1);
        assert_event_data_eq!(got[1].0, expected[1].0);
        assert_eq!(got[1].1, expected[1].1);
        assert_event_data_eq!(got[2].0, expected[2].0);
        assert_eq!(got[2].1, expected[2].1);
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
            (
                LogEvent::from(btreemap! {
                    "message" => "foo",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
                }),
                82,
            ),
            (
                LogEvent::from(btreemap! {
                    "message" => "bar",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:05Z").unwrap().into()),
                }),
                82,
            ),
            (
                LogEvent::from(btreemap! {
                    "message" => "baz",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:06Z").unwrap().into()),
                }),
                82,
            ),
        ];

        let got = decode_all(message).unwrap();

        assert_event_data_eq!(got[0].0, expected[0].0);
        assert_eq!(got[0].1, expected[0].1);
        assert_event_data_eq!(got[1].0, expected[1].0);
        assert_eq!(got[1].1, expected[1].1);
        assert_event_data_eq!(got[2].0, expected[2].0);
        assert_eq!(got[2].1, expected[2].1);
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
            (
                LogEvent::from(btreemap! {
                    "message" => "foo",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:04Z").unwrap().into()),
                }),
                84,
            ),
            (
                LogEvent::from(btreemap! {
                    "message" => "bar",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:05Z").unwrap().into()),
                }),
                84,
            ),
            (
                LogEvent::from(btreemap! {
                    "message" => "baz",
                    "tag" => "tag.name",
                    "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2015-09-07T01:23:06Z").unwrap().into()),
                }),
                84,
            ),
        ];

        let got = decode_all(message).unwrap();

        assert_event_data_eq!(got[0].0, expected[0].0);
        assert_eq!(got[0].1, expected[0].1);
        assert_event_data_eq!(got[1].0, expected[1].0);
        assert_eq!(got[1].1, expected[1].1);
        assert_event_data_eq!(got[2].0, expected[2].0);
        assert_eq!(got[2].1, expected[2].1);
    }

    fn decode_all(message: Vec<u8>) -> Result<Vec<(LogEvent, usize)>, DecodeError> {
        let mut buf = BytesMut::from(&message[..]);

        let mut decoder = FluentDecoder::new();

        let mut frames = vec![];
        while let Some((frame, byte_size)) = decoder.decode(&mut buf)? {
            frames.push((LogEvent::from(frame), byte_size))
        }
        Ok(frames)
    }
}

#[cfg(all(test, feature = "fluent-integration-tests"))]
mod integration_tests {
    use crate::config::SourceConfig;
    use crate::config::SourceContext;
    use crate::docker::docker;
    use crate::sources::fluent::FluentConfig;
    use crate::test_util::{collect_ready, next_addr_for_ip, trace_init, wait_for_tcp};
    use crate::Pipeline;
    use bollard::{
        container::{Config as ContainerConfig, CreateContainerOptions},
        image::{CreateImageOptions, ListImagesOptions},
        models::HostConfig,
        Docker,
    };
    use futures::{channel::mpsc, StreamExt};
    use std::{collections::HashMap, fs::File, io::Write, net::SocketAddr, time::Duration};
    use tokio::time::sleep;
    use uuid::Uuid;
    use vector_core::event::Event;

    #[tokio::test]
    async fn fluentbit() {
        trace_init();

        let image = "fluent/fluent-bit";
        let tag = "1.7";

        let docker = docker(None, None).unwrap();

        let (out, address) = source().await;

        pull_image(&docker, image, tag).await;

        let dir = tempfile::tempdir().unwrap();
        let mut file = File::create(dir.path().join("fluent-bit.conf")).unwrap();
        write!(
            &mut file,
            r#"
[SERVICE]
    Grace      0
    Flush      1
    Daemon     off

[INPUT]
    Name       dummy

[OUTPUT]
    Name          forward
    Match         *
    Host          host.docker.internal
    Port          {}
"#,
            address.port()
        )
        .unwrap();

        let options = Some(CreateContainerOptions {
            name: format!("vector_test_fluent_{}", Uuid::new_v4()),
        });
        let config = ContainerConfig {
            image: Some(format!("{}:{}", image, tag)),
            host_config: Some(HostConfig {
                network_mode: Some(String::from("host")),
                extra_hosts: Some(vec![String::from("host.docker.internal:host-gateway")]),
                binds: Some(vec![format!(
                    "{}:{}",
                    dir.path().display(),
                    "/fluent-bit/etc"
                )]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let container = docker.create_container(options, config).await.unwrap();

        docker
            .start_container::<String>(&container.id, None)
            .await
            .unwrap();

        sleep(Duration::from_secs(2)).await;

        let events = collect_ready(out).await;

        remove_container(&docker, &container.id).await;

        assert!(!events.is_empty());
        assert_eq!(events[0].as_log()["tag"], "dummy.0".into());
        assert_eq!(events[0].as_log()["message"], "dummy".into());
        assert!(events[0].as_log().get("timestamp").is_some());
        assert!(events[0].as_log().get("host").is_some());
    }

    #[tokio::test]
    async fn fluentd() {
        let config = r#"
<source>
  @type dummy
  dummy {"message": "dummy"}
  tag dummy
</source>

<match *>
  @type forward
  <server>
    name  local
    host  host.docker.internal
    port  PORT
  </server>
  <buffer>
    flush_mode immediate
  </buffer>
</match>
"#;
        test_fluentd(config).await;
    }

    #[tokio::test]
    async fn fluentd_gzip() {
        let config = r#"
<source>
  @type dummy
  dummy {"message": "dummy"}
  tag dummy
</source>

<match *>
  @type forward
  <server>
    name  local
    host  host.docker.internal
    port  PORT
  </server>
  <buffer>
    flush_mode immediate
  </buffer>
  compress gzip
</match>
"#;
        test_fluentd(config).await;
    }

    async fn test_fluentd(config: &str) {
        trace_init();

        let image = "fluent/fluentd";
        let tag = "v1.12";

        let docker = docker(None, None).unwrap();

        let (out, address) = source().await;

        pull_image(&docker, image, tag).await;

        let dir = tempfile::tempdir().unwrap();
        let mut file = File::create(dir.path().join("fluent.conf")).unwrap();
        write!(
            &mut file,
            "{}",
            config.replace("PORT", &address.port().to_string())
        )
        .unwrap();

        let options = Some(CreateContainerOptions {
            name: format!("vector_test_fluent_{}", Uuid::new_v4()),
        });
        let config = ContainerConfig {
            image: Some(format!("{}:{}", image, tag)),
            host_config: Some(HostConfig {
                network_mode: Some(String::from("host")),
                extra_hosts: Some(vec![String::from("host.docker.internal:host-gateway")]),
                binds: Some(vec![format!("{}:{}", dir.path().display(), "/fluentd/etc")]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let container = docker.create_container(options, config).await.unwrap();

        docker
            .start_container::<String>(&container.id, None)
            .await
            .unwrap();

        sleep(Duration::from_secs(5)).await;

        let events = collect_ready(out).await;

        remove_container(&docker, &container.id).await;

        assert!(!events.is_empty());
        assert_eq!(events[0].as_log()["tag"], "dummy".into());
        assert_eq!(events[0].as_log()["message"], "dummy".into());
        assert!(events[0].as_log().get("timestamp").is_some());
        assert!(events[0].as_log().get("host").is_some());
    }

    async fn pull_image(docker: &Docker, image: &str, tag: &str) {
        let mut filters = HashMap::new();
        filters.insert(
            String::from("reference"),
            vec![format!("{}:{}", image, tag)],
        );

        let options = Some(ListImagesOptions {
            filters,
            ..Default::default()
        });

        let images = docker.list_images(options).await.unwrap();
        if images.is_empty() {
            // If not found, pull it
            let options = Some(CreateImageOptions {
                from_image: image,
                tag,
                ..Default::default()
            });

            docker
                .create_image(options, None, None)
                .for_each(|item| async move {
                    let info = item.unwrap();
                    if let Some(error) = info.error {
                        panic!("{:?}", error);
                    }
                })
                .await
        }
    }

    async fn source() -> (mpsc::Receiver<Event>, SocketAddr) {
        let (sender, recv) = Pipeline::new_test();
        let address = next_addr_for_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        tokio::spawn(async move {
            FluentConfig {
                address: address.into(),
                tls: None,
                keepalive: None,
                receive_buffer_bytes: None,
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

    async fn remove_container(docker: &Docker, id: &str) {
        trace!("Stopping container.");

        let _ = docker
            .stop_container(id, None)
            .await
            .map_err(|e| error!(%e));

        trace!("Removing container.");

        // Don't panic, as this is unrelated to the test
        let _ = docker
            .remove_container(id, None)
            .await
            .map_err(|e| error!(%e));
    }
}
