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

    fn build_events(&self, frame: FluentFrame, host: Bytes) -> Option<Vec<Event>> {
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
                log.insert("host", host.clone());
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
