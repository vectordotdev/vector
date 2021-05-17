use super::util::{SocketListenAddr, TcpIsErrorFatal, TcpSource};
use crate::{
    config::{DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceDescription},
    event::Event,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
};
use bytes::{Bytes, BytesMut};
use rmp_serde::{decode, Deserializer};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};
use tokio_util::codec::Decoder;

// TODO
// * internal events

#[derive(Deserialize, Serialize, Debug)]
pub struct FluentConfig {
    address: SocketListenAddr,
    tls: Option<TlsConfig>,
    keepalive: Option<TcpKeepaliveConfig>,
    receive_buffer_bytes: Option<usize>,
    // TODO
    //tag: Option<String>,
    //add_tag_prefix: Option<String>,
    // TODO authentication, compression, acking
}

inventory::submit! {
    SourceDescription::new::<FluentConfig>("fluent")
}

impl GenerateConfig for FluentConfig {
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

    fn build_event(&self, _fluent_message: FluentMessage, _host: Bytes) -> Option<Event> {
        None
    }
}

#[derive(Debug)]
pub enum DecodeError {
    IO(io::Error),
    Decode(decode::Error),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::IO(err) => write!(f, "{}", err),
            DecodeError::Decode(err) => write!(f, "{}", err),
        }
    }
}

impl TcpIsErrorFatal for DecodeError {
    fn is_error_fatal(&self) -> bool {
        match self {
            DecodeError::IO(_) => true,
            DecodeError::Decode(_) => false, // TODO is this right?
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

#[derive(Debug, Deserialize)]
struct FluentMessage(String, Vec<FluentRecord>);

#[derive(Debug, Deserialize)]
struct FluentRecord {
    timestamp: String,
    fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
struct FluentDecoder;

impl Decoder for FluentDecoder {
    type Item = FluentMessage;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() == 0 {
            return Ok(None);
        }
        dbg!(&src);
        let (pos, rv) = {
            let mut des = Deserializer::new(io::Cursor::new(&src[..]));
            let rv = Deserialize::deserialize(&mut des).map_err(|e| DecodeError::Decode(e));
            if let Err(DecodeError::Decode(decode::Error::InvalidDataRead(ref custom))) = rv {
                if custom.kind() == io::ErrorKind::UnexpectedEof {
                    return Ok(None);
                }
            }
            let pos = des.position() as usize;
            (pos, rv)
        };
        src.split_to(pos);
        rv
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
