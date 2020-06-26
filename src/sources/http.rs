use crate::{
    event::{self, Event},
    shutdown::ShutdownSignal,
    sources::util::{ErrorMessage, HttpSource},
    tls::TlsConfig,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes05::Bytes;
use chrono::Utc;
use codec::{self, BytesDelimitedCodec};
use futures01::sync::mpsc;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::net::SocketAddr;
use tokio_codec::Decoder;
use warp::http::{HeaderMap, HeaderValue, StatusCode};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SimpleHttpConfig {
    address: SocketAddr,
    #[serde(default)]
    encoding: Encoding,
    #[serde(default)]
    headers: Vec<String>,
    tls: Option<TlsConfig>,
}

inventory::submit! {
    SourceDescription::new_without_default::<SimpleHttpConfig>("http")
}

#[derive(Clone)]
struct SimpleHttpSource {
    encoding: Encoding,
    headers: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative, Copy)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Ndjson,
    Json,
}

impl HttpSource for SimpleHttpSource {
    fn build_event(&self, body: Bytes, header_map: HeaderMap) -> Result<Vec<Event>, ErrorMessage> {
        decode_body(body, self.encoding)
            .map(|events| add_headers(events, &self.headers, header_map))
            .map(|mut events| {
                // Add source type
                let key = event::log_schema().source_type_key();
                for event in events.iter_mut() {
                    event.as_mut_log().try_insert(key, "http");
                }
                events
            })
    }
}

#[typetag::serde(name = "http")]
impl SourceConfig for SimpleHttpConfig {
    fn build(
        &self,
        _: &str,
        _: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let source = SimpleHttpSource {
            encoding: self.encoding,
            headers: self.headers.clone(),
        };
        source.run(self.address, "", &self.tls, out, shutdown)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "http"
    }
}

fn add_headers(
    mut events: Vec<Event>,
    headers_config: &[String],
    headers: HeaderMap,
) -> Vec<Event> {
    for header_name in headers_config {
        let value = headers
            .get(header_name)
            .map(HeaderValue::as_bytes)
            .unwrap_or_default();
        for event in events.iter_mut() {
            event.as_mut_log().insert(header_name as &str, value);
        }
    }

    events
}

fn body_to_lines(buf: Bytes) -> impl Iterator<Item = Result<bytes::Bytes, ErrorMessage>> {
    // TODO: remove on bytes 0.4 => 0.5 update
    let mut body = bytes::BytesMut::new();
    body.extend_from_slice(&buf);

    let mut decoder = BytesDelimitedCodec::new(b'\n');
    std::iter::from_fn(move || {
        match decoder.decode_eof(&mut body) {
            Err(e) => Some(Err(ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Bad request: {}", e),
            ))),
            Ok(Some(b)) => Some(Ok(b)),
            Ok(None) => None, //actually done
        }
    })
    .filter(|s| match s {
        //filter empty lines
        Ok(b) => !b.is_empty(),
        _ => true,
    })
}

fn decode_body(body: Bytes, enc: Encoding) -> Result<Vec<Event>, ErrorMessage> {
    match enc {
        Encoding::Text => body_to_lines(body)
            .map(|r| Ok(Event::from(r?)))
            .collect::<Result<_, _>>(),
        Encoding::Ndjson => body_to_lines(body)
            .map(|j| {
                let parsed_json = serde_json::from_slice(&j?)
                    .map_err(|e| json_error(format!("Error parsing Ndjson: {:?}", e)))?;
                json_parse_object(parsed_json)
            })
            .collect::<Result<_, _>>(),
        Encoding::Json => {
            let parsed_json = serde_json::from_slice(&body)
                .map_err(|e| json_error(format!("Error parsing Json: {:?}", e)))?;
            json_parse_array_of_object(parsed_json)
        }
    }
}

fn json_parse_object(value: JsonValue) -> Result<Event, ErrorMessage> {
    let mut event = Event::new_empty_log();
    let log = event.as_mut_log();
    log.insert(event::log_schema().timestamp_key().clone(), Utc::now()); // Add timestamp
    match value {
        JsonValue::Object(map) => {
            for (k, v) in map {
                log.insert(k, v);
            }
            Ok(event)
        }
        _ => Err(json_error(format!(
            "Expected Object, got {}",
            json_value_to_type_string(&value)
        ))),
    }
}

fn json_parse_array_of_object(value: JsonValue) -> Result<Vec<Event>, ErrorMessage> {
    match value {
        JsonValue::Array(v) => v
            .into_iter()
            .map(json_parse_object)
            .collect::<Result<_, _>>(),
        JsonValue::Object(map) => {
            //treat like an array of one object
            Ok(vec![json_parse_object(JsonValue::Object(map))?])
        }
        _ => Err(json_error(format!(
            "Expected Array or Object, got {}.",
            json_value_to_type_string(&value)
        ))),
    }
}

fn json_error(s: String) -> ErrorMessage {
    ErrorMessage::new(StatusCode::BAD_REQUEST, format!("Bad JSON: {}", s))
}

fn json_value_to_type_string(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Object(_) => "Object",
        JsonValue::Array(_) => "Array",
        JsonValue::String(_) => "String",
        JsonValue::Number(_) => "Number",
        JsonValue::Bool(_) => "Bool",
        JsonValue::Null => "Null",
    }
}

#[cfg(test)]
mod tests {
    use super::{Encoding, SimpleHttpConfig};

    use crate::shutdown::ShutdownSignal;
    use crate::{
        event::{self, Event},
        runtime::Runtime,
        test_util::{self, collect_n, runtime},
        topology::config::{GlobalOptions, SourceConfig},
    };
    use futures01::sync::mpsc;
    use http01::{HeaderMap, Method};
    use pretty_assertions::assert_eq;
    use std::net::SocketAddr;
    use string_cache::DefaultAtom as Atom;

    fn source(
        rt: &mut Runtime,
        encoding: Encoding,
        headers: Vec<String>,
    ) -> (mpsc::Receiver<Event>, SocketAddr) {
        test_util::trace_init();
        let (sender, recv) = mpsc::channel(100);
        let address = test_util::next_addr();
        rt.spawn(
            SimpleHttpConfig {
                address,
                encoding,
                headers,
                tls: None,
            }
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
            .unwrap(),
        );
        (recv, address)
    }

    fn send(address: SocketAddr, body: &str) -> u16 {
        reqwest::Client::new()
            .post(&format!("http://{}/", address))
            .body(body.to_owned())
            .send()
            .unwrap()
            .status()
            .as_u16()
    }

    fn send_with_headers(address: SocketAddr, body: &str, headers: HeaderMap) -> u16 {
        reqwest::Client::new()
            .request(Method::POST, &format!("http://{}/", address))
            .headers(headers)
            .body(body.to_owned())
            .send()
            .unwrap()
            .status()
            .as_u16()
    }

    #[test]
    fn http_multiline_text() {
        let body = "test body\n\ntest body 2";

        let mut rt = runtime();
        let (rx, addr) = source(&mut rt, Encoding::default(), vec![]);

        assert_eq!(200, send(addr, body));

        let mut events = rt.block_on(collect_n(rx, 2)).unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&event::log_schema().message_key()], "test body".into());
            assert!(log.get(&event::log_schema().timestamp_key()).is_some());
            assert_eq!(log[event::log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(
                log[&event::log_schema().message_key()],
                "test body 2".into()
            );
            assert!(log.get(&event::log_schema().timestamp_key()).is_some());
            assert_eq!(log[event::log_schema().source_type_key()], "http".into());
        }
    }

    #[test]
    fn http_multiline_text2() {
        //same as above test but with a newline at the end
        let body = "test body\n\ntest body 2\n";

        let mut rt = runtime();
        let (rx, addr) = source(&mut rt, Encoding::default(), vec![]);

        assert_eq!(200, send(addr, body));

        let mut events = rt.block_on(collect_n(rx, 2)).unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&event::log_schema().message_key()], "test body".into());
            assert!(log.get(&event::log_schema().timestamp_key()).is_some());
            assert_eq!(log[event::log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(
                log[&event::log_schema().message_key()],
                "test body 2".into()
            );
            assert!(log.get(&event::log_schema().timestamp_key()).is_some());
            assert_eq!(log[event::log_schema().source_type_key()], "http".into());
        }
    }

    #[test]
    fn http_json_parsing() {
        let mut rt = runtime();
        let (rx, addr) = source(&mut rt, Encoding::Json, vec![]);

        assert_eq!(400, send(addr, "{")); //malformed
        assert_eq!(400, send(addr, r#"{"key"}"#)); //key without value

        assert_eq!(200, send(addr, "{}")); //can be one object or array of objects
        assert_eq!(200, send(addr, "[{},{},{}]"));

        let mut events = rt.block_on(collect_n(rx, 2)).unwrap();
        assert!(events
            .remove(1)
            .as_log()
            .get(&event::log_schema().timestamp_key())
            .is_some());
        assert!(events
            .remove(0)
            .as_log()
            .get(&event::log_schema().timestamp_key())
            .is_some());
    }

    #[test]
    fn http_json_values() {
        let mut rt = runtime();
        let (rx, addr) = source(&mut rt, Encoding::Json, vec![]);

        assert_eq!(200, send(addr, r#"[{"key":"value"}]"#));
        assert_eq!(200, send(addr, r#"{"key2":"value2"}"#));

        let mut events = rt.block_on(collect_n(rx, 2)).unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key")], "value".into());
            assert!(log.get(&event::log_schema().timestamp_key()).is_some());
            assert_eq!(log[event::log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key2")], "value2".into());
            assert!(log.get(&event::log_schema().timestamp_key()).is_some());
            assert_eq!(log[event::log_schema().source_type_key()], "http".into());
        }
    }

    #[test]
    fn http_ndjson() {
        let mut rt = runtime();
        let (rx, addr) = source(&mut rt, Encoding::Ndjson, vec![]);

        assert_eq!(400, send(addr, r#"[{"key":"value"}]"#)); //one object per line

        assert_eq!(
            200,
            send(addr, "{\"key1\":\"value1\"}\n\n{\"key2\":\"value2\"}")
        );

        let mut events = rt.block_on(collect_n(rx, 2)).unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key1")], "value1".into());
            assert!(log.get(&event::log_schema().timestamp_key()).is_some());
            assert_eq!(log[event::log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key2")], "value2".into());
            assert!(log.get(&event::log_schema().timestamp_key()).is_some());
            assert_eq!(log[event::log_schema().source_type_key()], "http".into());
        }
    }

    #[test]
    fn http_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", "test_client".parse().unwrap());
        headers.insert("Upgrade-Insecure-Requests", "false".parse().unwrap());

        let mut rt = runtime();
        let (rx, addr) = source(
            &mut rt,
            Encoding::Ndjson,
            vec![
                "User-Agent".to_string(),
                "Upgrade-Insecure-Requests".to_string(),
                "AbsentHeader".to_string(),
            ],
        );

        assert_eq!(
            200,
            send_with_headers(addr, "{\"key1\":\"value1\"}", headers)
        );

        let mut events = rt.block_on(collect_n(rx, 1)).unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key1")], "value1".into());
            assert_eq!(log[&Atom::from("User-Agent")], "test_client".into());
            assert_eq!(
                log[&Atom::from("Upgrade-Insecure-Requests")],
                "false".into()
            );
            assert_eq!(log[&Atom::from("AbsentHeader")], "".into());
            assert!(log.get(&event::log_schema().timestamp_key()).is_some());
            assert_eq!(log[event::log_schema().source_type_key()], "http".into());
        }
    }
}
