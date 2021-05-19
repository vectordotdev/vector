use super::util::{SocketListenAddr, TcpIsErrorFatal, TcpSource};
use crate::{
    config::{DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceDescription},
    event::{Event, Value},
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
};
use bytes::{Buf, Bytes, BytesMut};
use chrono::{serde::ts_seconds, DateTime, TimeZone, Utc};
use flate2::read::MultiGzDecoder;
use rmp_serde::{decode, Deserializer};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    convert::TryInto,
    io::{self, Read},
};
use tokio_util::codec::Decoder;

// TODO
// * internal events
// * consider using serde_tuple
// * authentication
// * chunking/acking
// * support tag and tag prefix overrides
// * integration testing

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
    type Decoder = FluentDecoder;

    fn decoder(&self) -> Self::Decoder {
        FluentDecoder
    }

    fn build_events(&self, frame: FluentFrame, _host: Bytes) -> Option<Vec<Event>> {
        let FluentFrame { tag, entries } = frame;

        let events = entries
            .into_iter()
            .map(|FluentEntry(timestamp, record)| {
                let fields = record
                    .into_iter()
                    .map(|(key, value)| (key, Value::from(value)))
                    .collect::<BTreeMap<String, Value>>();
                let mut event = Event::from(fields);
                let log = event.as_mut_log();
                log.insert("timestamp", timestamp.clone());
                log.insert("fluent_tag", tag.clone());
                event
            })
            .collect();

        Some(events)
    }
}

#[derive(Debug)]
pub enum DecodeError {
    IO(io::Error),
    Decode(decode::Error),
    UnknownCompression(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::IO(err) => write!(f, "{}", err),
            DecodeError::Decode(err) => write!(f, "{}", err),
            DecodeError::UnknownCompression(compression) => {
                write!(f, "unknown compression: {}", compression)
            }
        }
    }
}

impl TcpIsErrorFatal for DecodeError {
    fn is_error_fatal(&self) -> bool {
        match self {
            DecodeError::IO(_) => true,
            DecodeError::Decode(_) => false,
            DecodeError::UnknownCompression(_) => false,
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

#[derive(Clone, Debug)]
struct FluentDecoder;

impl Decoder for FluentDecoder {
    type Item = FluentFrame;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() == 0 {
            return Ok(None);
        }
        dbg!(&src);
        dbg!(base64::encode(&src));
        let (pos, res) = {
            let mut des = Deserializer::new(io::Cursor::new(&src[..]));

            // attempt to parse, if we get unexpected EOF, we need more data
            let res = Deserialize::deserialize(&mut des).map_err(|e| DecodeError::Decode(e));
            dbg!(&res);
            if let Err(DecodeError::Decode(decode::Error::InvalidDataRead(ref custom))) = res {
                if custom.kind() == io::ErrorKind::UnexpectedEof {
                    return Ok(None);
                }
            }

            (des.position() as usize, res)
        };

        src.advance(pos);

        let message = res?;

        let res = match message {
            FluentMessage::Message(tag, timestamp, record)
            | FluentMessage::MessageWithOptions(tag, timestamp, record, ..) => Ok(FluentFrame {
                tag,
                entries: vec![FluentEntry(timestamp, record)],
            }),
            FluentMessage::Forward(tag, entries)
            | FluentMessage::ForwardWithOptions(tag, entries, ..) => {
                Ok(FluentFrame { tag, entries })
            }
            FluentMessage::PackedForward(tag, bin) => {
                let mut buf = BytesMut::from(&bin[..]);

                dbg!(base64::encode(&buf));

                let mut decoder = FluentEntryStreamDecoder;

                let mut entries = Vec::new();
                while let Some(entry) = decoder.decode(&mut buf)? {
                    entries.push(entry);
                }
                Ok(FluentFrame { tag, entries })
            }
            FluentMessage::PackedForwardWithOptions(tag, bin, options) => {
                let buf = match options.compressed.as_str() {
                    "gzip" => {
                        let mut buf = Vec::new();
                        MultiGzDecoder::new(io::Cursor::new(bin.into_vec()))
                            .read_to_end(&mut buf)
                            .map(|_| buf)
                            .map_err(Into::into)
                    }
                    "text" => Ok(bin.into_vec()),
                    s => Err(DecodeError::UnknownCompression(s.to_owned())),
                }?;

                let mut buf = BytesMut::from(&buf[..]);

                dbg!(base64::encode(&buf));

                let mut decoder = FluentEntryStreamDecoder;

                let mut entries = Vec::new();
                while let Some(entry) = decoder.decode(&mut buf)? {
                    entries.push(entry);
                }
                Ok(FluentFrame { tag, entries })
            }
        };

        res.map(Option::Some)
    }
}

/// Decoder for decoding MessagePackEventStream which are just a stream of entries
#[derive(Clone, Debug)]
struct FluentEntryStreamDecoder;

impl Decoder for FluentEntryStreamDecoder {
    type Item = FluentEntry;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() == 0 {
            return Ok(None);
        }
        dbg!(&src);
        dbg!(base64::encode(&src));
        let (pos, res) = {
            let mut des = Deserializer::new(io::Cursor::new(&src[..]));

            // attempt to parse, if we get unexpected EOF, we need more data
            let res = Deserialize::deserialize(&mut des).map_err(|e| DecodeError::Decode(e));
            dbg!(&res);
            if let Err(DecodeError::Decode(decode::Error::InvalidDataRead(ref custom))) = res {
                if custom.kind() == io::ErrorKind::UnexpectedEof {
                    return Ok(None);
                }
            }

            (des.position() as usize, res)
        };

        src.advance(pos);

        res
    }
}

/// Normalized fluent message.
struct FluentFrame {
    tag: FluentTag,
    entries: Vec<FluentEntry>,
}

/// Fluent msgpack messages can be encoded in one of three ways, each with and without
/// options, all using arrays to encode the top-level fields.
///
/// The spec refers to 4 ways, but really CompressedPackedForward is encoded the same as
/// PackedForward, it just has an additional decompression step.
///
/// Not yet handled are the handshake messages.
///
/// https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#event-modes
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FluentMessage {
    Message(FluentTag, FluentTimestamp, FluentRecord),
    // I attempted to just one variant for each of these, with and without options, using an
    // `Option` for the last element, but rmp expected the number of elements to match in that case
    // still (it just allows the last element to be `nil`).
    MessageWithOptions(
        FluentTag,
        FluentTimestamp,
        FluentRecord,
        FluentMessageOptions,
    ),
    Forward(FluentTag, Vec<FluentEntry>),
    ForwardWithOptions(FluentTag, Vec<FluentEntry>, FluentMessageOptions),
    PackedForward(FluentTag, serde_bytes::ByteBuf),
    PackedForwardWithOptions(FluentTag, serde_bytes::ByteBuf, FluentMessageOptions),
}

/// Server options sent by client.
///
/// https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#option
#[derive(Debug, Deserialize)]
struct FluentMessageOptions {
    size: Option<u64>,
    chunk: Option<String>,
    compressed: String, // this one is required if present
}

/// Fluent entry consisting of timestamp and record.
///
/// https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#forward-mode
#[derive(Debug, Deserialize)]
struct FluentEntry(FluentTimestamp, FluentRecord);

/// Fluent record is just key/value pairs.
type FluentRecord = BTreeMap<String, FluentValue>;

/// Fluent message tag.
type FluentTag = String;

/// Value for fluent record key.
///
/// Used mostly just to implement value conversion.
#[derive(Debug, Deserialize)]
struct FluentValue(rmpv::Value);

impl From<FluentValue> for Value {
    fn from(value: FluentValue) -> Self {
        match value.0 {
            rmpv::Value::Nil => Value::Null,
            rmpv::Value::Boolean(b) => Value::Boolean(b),
            rmpv::Value::Integer(i) => i
                .as_i64()
                .map(|i| Value::Integer(i))
                // unwrap large numbers to string similar to how `From<serde_json::Value> for Value` handles it
                .unwrap_or_else(|| Value::Bytes(i.to_string().into())),
            rmpv::Value::F32(f) => Value::Float(f.into()),
            rmpv::Value::F64(f) => Value::Float(f),
            rmpv::Value::String(s) => Value::Bytes(s.into_bytes().into()),
            rmpv::Value::Binary(bytes) => Value::Bytes(bytes.into()),
            rmpv::Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(|value| Value::from(FluentValue(value)))
                    .collect(),
            ),
            rmpv::Value::Map(values) => Value::Map(
                values
                    .into_iter()
                    .map(|(key, value)| (format!("{}", key), Value::from(FluentValue(value))))
                    .collect(),
            ),
            rmpv::Value::Ext(code, bytes) => {
                let mut fields = BTreeMap::new();
                fields.insert(
                    String::from("msgpack_extension_code"),
                    Value::Integer(code.into()),
                );
                fields.insert(String::from("bytes"), Value::Bytes(bytes.into()));
                Value::Map(fields)
            }
        }
    }
}

/// Fluent message timestamp.
///
/// Message timestamps can be a unix timestamp or EventTime messagepack ext.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum FluentTimestamp {
    #[serde(with = "ts_seconds")]
    Unix(DateTime<Utc>),
    Ext(FluentEventTime),
}

impl From<FluentTimestamp> for Value {
    fn from(timestamp: FluentTimestamp) -> Self {
        match timestamp {
            FluentTimestamp::Unix(timestamp) | FluentTimestamp::Ext(FluentEventTime(timestamp)) => {
                Value::Timestamp(timestamp)
            }
        }
    }
}

/// Custom decoder for Fluent's EventTime msgpack extension.
///
/// https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#eventtime-ext-format
#[derive(Clone, Debug)]
struct FluentEventTime(DateTime<Utc>);

impl<'de> serde::de::Deserialize<'de> for FluentEventTime {
    fn deserialize<D>(deserializer: D) -> Result<FluentEventTime, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct FluentEventTimeVisitor;

        impl<'de> serde::de::Visitor<'de> for FluentEventTimeVisitor {
            type Value = FluentEventTime;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("fluent timestamp extension")
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::de::Deserializer<'de>,
            {
                deserializer.deserialize_tuple(2, self)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let tag: u32 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;

                if tag != 0 {
                    return Err(serde::de::Error::custom(format!(
                        "expected extension type 0 for fluent timestamp, got got {}",
                        tag
                    )));
                }

                let bytes: serde_bytes::ByteBuf = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;

                if bytes.len() != 8 {
                    return Err(serde::de::Error::custom(format!(
                        "expected exactly 8 bytes for binary encoded fluent timestamp, got {}",
                        bytes.len()
                    )));
                }

                // length checked right above
                let seconds = u32::from_be_bytes(bytes[..4].try_into().expect("exactly 4 bytes"));
                let nanoseconds =
                    u32::from_be_bytes(bytes[4..].try_into().expect("exactly 4 bytes"));

                Ok(FluentEventTime(Utc.timestamp(seconds.into(), nanoseconds)))
            }
        }

        deserializer.deserialize_any(FluentEventTimeVisitor)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{config::log_schema, event::Event};
    use bytes::BufMut;
    use chrono::prelude::*;
    use shared::assert_event_data_eq;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<FluentConfig>();
    }

    #[test]
    fn config_tcp() {
        let config: FluentConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(config.mode.is_tcp());
    }

    #[test]
    fn config_tcp_with_receive_buffer_size() {
        let config: FluentConfig = toml::from_str(
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
        let config: FluentConfig = toml::from_str(
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
        let config: FluentConfig = toml::from_str(
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
        let config: FluentConfig = toml::from_str(
            r#"
            mode = "udp"
            address = "127.0.0.1:1235"
            max_length = 32187
          "#,
        )
        .unwrap();
        assert!(config.mode.is_udp());
    }

    #[test]
    fn config_udp_with_receive_buffer_size() {
        let config: FluentConfig = toml::from_str(
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
        let config: FluentConfig = toml::from_str(
            r#"
            mode = "unix"
            path = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(config.mode.is_unix());
    }

    #[test]
    fn fluent_ng_network_fluent_protocol() {
        // this should also match rfluent omfwd with template=RFLUENT_FluentProtocol23Format
        let msg = "i am foobar";
        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {}{} {}"#,
            r#"[meta sequenceId="1" sysUpTime="37" language="EN"]"#,
            r#"[origin ip="192.168.0.1" software="test"]"#,
            msg
        );

        let mut expected = Event::from(msg);

        {
            let expected = expected.as_mut_log();
            expected.insert(
                log_schema().timestamp_key(),
                chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34),
            );
            expected.insert(log_schema().source_type_key(), "fluent");
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
        }

        assert_event_data_eq!(event_from_str(&"host".to_string(), None, &raw), expected);
    }

    #[test]
    fn handles_incorrect_sd_element() {
        let msg = "qwerty";
        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} {}"#,
            r#"[incorrect x]"#, msg
        );

        let mut expected = Event::from(msg);
        {
            let expected = expected.as_mut_log();
            expected.insert(
                log_schema().timestamp_key(),
                chrono::Utc.ymd(2019, 2, 13).and_hms(19, 48, 34),
            );
            expected.insert(log_schema().host_key(), "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");
            expected.insert(log_schema().source_type_key(), "fluent");
            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("version", 1);
            expected.insert("appname", "root");
            expected.insert("procid", 8449);
        }

        let event = event_from_str(&"host".to_string(), None, &raw);
        assert_event_data_eq!(event, expected);

        let raw = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} {}"#,
            r#"[incorrect x=]"#, msg
        );

        let event = event_from_str(&"host".to_string(), None, &raw);
        assert_event_data_eq!(event, expected);
    }

    #[test]
    fn handles_empty_sd_element() {
        fn there_is_map_called_empty(event: Event) -> bool {
            event
                .as_log()
                .all_fields()
                .find(|(key, _)| (&key[..]).starts_with("empty"))
                == None
        }

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg);
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[non_empty x="1"][empty]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg);
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty][non_empty x="1"]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg);
        assert!(there_is_map_called_empty(event));

        let msg = format!(
            r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - {} qwerty"#,
            r#"[empty not_really="testing the test"]"#
        );

        let event = event_from_str(&"host".to_string(), None, &msg);
        assert!(!there_is_map_called_empty(event));
    }

    #[test]
    fn handles_weird_whitespace() {
        // this should also match rfluent omfwd with template=RFLUENT_FluentProtocol23Format
        let raw = r#"
            <13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar
            "#;
        let cleaned = r#"<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar"#;

        assert_event_data_eq!(
            event_from_str(&"host".to_string(), None, raw),
            event_from_str(&"host".to_string(), None, cleaned)
        );
    }

    #[test]
    fn fluent_ng_default_network() {
        let msg = "i am foobar";
        let raw = format!(r#"<13>Feb 13 20:07:26 74794bfb6795 root[8539]: {}"#, msg);
        let event = event_from_str(&"host".to_string(), None, &raw);

        let mut expected = Event::from(msg);
        {
            let value = event.as_log().get("timestamp").unwrap();
            let year = value.as_timestamp().unwrap().naive_local().year();

            let expected = expected.as_mut_log();
            let expected_date: DateTime<Utc> =
                chrono::Local.ymd(year, 2, 13).and_hms(20, 7, 26).into();
            expected.insert(log_schema().timestamp_key(), expected_date);
            expected.insert(log_schema().host_key(), "74794bfb6795");
            expected.insert(log_schema().source_type_key(), "fluent");
            expected.insert("hostname", "74794bfb6795");
            expected.insert("severity", "notice");
            expected.insert("facility", "user");
            expected.insert("appname", "root");
            expected.insert("procid", 8539);
        }

        assert_event_data_eq!(event, expected);
    }

    #[test]
    fn rfluent_omfwd_tcp_default() {
        let msg = "start";
        let raw = format!(
            r#"<190>Feb 13 21:31:56 74794bfb6795 liblogging-stdlog:  [origin software="rfluentd" swVersion="8.24.0" x-pid="8979" x-info="http://www.rfluent.com"] {}"#,
            msg
        );
        let event = event_from_str(&"host".to_string(), None, &raw);

        let mut expected = Event::from(msg);
        {
            let value = event.as_log().get("timestamp").unwrap();
            let year = value.as_timestamp().unwrap().naive_local().year();

            let expected = expected.as_mut_log();
            let expected_date: DateTime<Utc> =
                chrono::Local.ymd(year, 2, 13).and_hms(21, 31, 56).into();
            expected.insert(log_schema().timestamp_key(), expected_date);
            expected.insert(log_schema().source_type_key(), "fluent");
            expected.insert("host", "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");
            expected.insert("severity", "info");
            expected.insert("facility", "local7");
            expected.insert("appname", "liblogging-stdlog");
            expected.insert("origin.software", "rfluentd");
            expected.insert("origin.swVersion", "8.24.0");
            expected.insert("origin.x-pid", "8979");
            expected.insert("origin.x-info", "http://www.rfluent.com");
        }

        assert_event_data_eq!(event, expected);
    }

    #[test]
    fn rfluent_omfwd_tcp_forward_format() {
        let msg = "start";
        let raw = format!(
            r#"<190>2019-02-13T21:53:30.605850+00:00 74794bfb6795 liblogging-stdlog:  [origin software="rfluentd" swVersion="8.24.0" x-pid="9043" x-info="http://www.rfluent.com"] {}"#,
            msg
        );

        let mut expected = Event::from(msg);
        {
            let expected = expected.as_mut_log();
            expected.insert(
                log_schema().timestamp_key(),
                chrono::Utc
                    .ymd(2019, 2, 13)
                    .and_hms_micro(21, 53, 30, 605_850),
            );
            expected.insert(log_schema().source_type_key(), "fluent");
            expected.insert("host", "74794bfb6795");
            expected.insert("hostname", "74794bfb6795");
            expected.insert("severity", "info");
            expected.insert("facility", "local7");
            expected.insert("appname", "liblogging-stdlog");
            expected.insert("origin.software", "rfluentd");
            expected.insert("origin.swVersion", "8.24.0");
            expected.insert("origin.x-pid", "9043");
            expected.insert("origin.x-info", "http://www.rfluent.com");
        }

        assert_event_data_eq!(event_from_str(&"host".to_string(), None, &raw), expected);
    }

    #[test]
    fn non_octet_decode_works_with_multiple_frames() {
        let mut decoder = FluentDecoder::new(128);
        let mut buffer = BytesMut::with_capacity(16);

        buffer.put(&b"<57>Mar 25 21:47:46 gleichner6005 quaerat[2444]: There were "[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(Ok(None), result.map_err(|_| true));

        buffer.put(&b"8 penguins in the shop.\n"[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(
            Ok(Some("<57>Mar 25 21:47:46 gleichner6005 quaerat[2444]: There were 8 penguins in the shop.".to_string())),
            result.map_err(|_| true)
        );
    }

    #[test]
    fn octet_decode_works_with_multiple_frames() {
        let mut decoder = FluentDecoder::new(30);
        let mut buffer = BytesMut::with_capacity(16);

        buffer.put(&b"28 abcdefghijklm"[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(Ok(None), result.map_err(|_| false));

        // Sending another frame starting with a number should not cause it to
        // try to decode a new message.
        buffer.put(&b"3 nopqrstuvwxyz"[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(
            Ok(Some("abcdefghijklm3 nopqrstuvwxyz".to_string())),
            result.map_err(|_| false)
        );
    }

    #[test]
    fn octet_decode_moves_past_invalid_length() {
        let mut decoder = FluentDecoder::new(16);
        let mut buffer = BytesMut::with_capacity(16);

        // An invalid fluent message that starts with a digit so we think it is starting with the len.
        buffer.put(&b"232>1 zork"[..]);
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b"zork"[..], buffer);
    }

    #[test]
    fn octet_decode_moves_past_invalid_utf8() {
        let mut decoder = FluentDecoder::new(16);
        let mut buffer = BytesMut::with_capacity(16);

        // An invalid fluent message containing invalid utf8 bytes.
        buffer.put(&[b'4', b' ', 0xf0, 0x28, 0x8c, 0xbc][..]);
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b""[..], buffer);
    }

    #[test]
    fn octet_decode_moves_past_exceeded_frame_length() {
        let mut decoder = FluentDecoder::new(16);
        let mut buffer = BytesMut::with_capacity(32);

        buffer.put(&b"32thisshouldbelongerthanthmaxframeasizewhichmeansthefluentparserwillnotbeabletodecodeit\n"[..]);
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b""[..], buffer);
    }

    #[test]
    fn octet_decode_rejects_exceeded_frame_length() {
        let mut decoder = FluentDecoder::new(16);
        let mut buffer = BytesMut::with_capacity(32);

        buffer.put(&b"26 abcdefghijklmnopqrstuvwxyzand here we are"[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(Ok(None), result.map_err(|_| false));
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b"and here we are"[..], buffer);
    }

    #[test]
    fn octet_decode_rejects_exceeded_frame_length_multiple_frames() {
        let mut decoder = FluentDecoder::new(16);
        let mut buffer = BytesMut::with_capacity(32);

        buffer.put(&b"26 abc"[..]);
        let _result = decoder.decode(&mut buffer);

        buffer.put(&b"defghijklmnopqrstuvwxyzand here we are"[..]);
        let result = decoder.decode(&mut buffer);

        println!("{:?}", result);
        assert!(result.is_err());
        assert_eq!(b"and here we are"[..], buffer);
    }

    #[test]
    fn octet_decode_moves_past_exceeded_frame_length_multiple_frames() {
        let mut decoder = FluentDecoder::new(16);
        let mut buffer = BytesMut::with_capacity(32);

        buffer.put(&b"32thisshouldbelongerthanthmaxframeasizewhichmeansthefluentparserwillnotbeabletodecodeit"[..]);
        let _ = decoder.decode(&mut buffer);

        assert_eq!(decoder.octet_decoding, Some(State::DiscardingToEol));
        buffer.put(&b"wemustcontinuetodiscard\n32 something valid"[..]);
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b"32 something valid"[..], buffer);
    }
}
