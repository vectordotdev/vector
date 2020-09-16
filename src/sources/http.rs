use crate::{
    config::{
        log_schema, DataType, GenerateConfig, GlobalOptions, SourceConfig, SourceDescription,
    },
    event::{Event, Value},
    shutdown::ShutdownSignal,
    sources::util::{ErrorMessage, HttpSource},
    tls::TlsConfig,
    Pipeline,
};
use bytes::{Bytes, BytesMut};
use chrono::Utc;
use codec::BytesDelimitedCodec;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::net::SocketAddr;
use string_cache::DefaultAtom as Atom;
use tokio_util::codec::Decoder;
use warp::http::{HeaderMap, HeaderValue, StatusCode};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SimpleHttpConfig {
    address: SocketAddr,
    #[serde(default)]
    encoding: Encoding,
    #[serde(default)]
    headers: Vec<String>,
    tls: Option<TlsConfig>,
    #[serde(default)]
    metrics_plus_mode: bool,
}

inventory::submit! {
    SourceDescription::new::<SimpleHttpConfig>("http")
}

impl GenerateConfig for SimpleHttpConfig {}

#[derive(Clone)]
struct SimpleHttpSource {
    encoding: Encoding,
    metrics_plus_mode: bool,
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
        decode_body(body, self.encoding, self.metrics_plus_mode)
            .map(|events| add_headers(events, &self.headers, header_map))
            .map(|mut events| {
                // Add source type
                let key = log_schema().source_type_key();
                for event in events.iter_mut() {
                    match event {
                        Event::Log(event) => event.try_insert(&Atom::from(key), Bytes::from("http")),
                        Event::Metric(_) => (),
                    }
                }
                events
            })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "http")]
impl SourceConfig for SimpleHttpConfig {
    async fn build(
        &self,
        _: &str,
        _: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        // Fail fast
        if self.metrics_plus_mode && self.encoding == Encoding::Text {
            panic!("HTTP Source: Metrics plus mode does not support text encoding")
        }
        let source = SimpleHttpSource {
            encoding: self.encoding,
            metrics_plus_mode: self.metrics_plus_mode,
            headers: self.headers.clone(),
        };
        source.run(self.address, "", &self.tls, out, shutdown)
    }

    fn output_type(&self) -> DataType {
        DataType::Any
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
            event.as_mut_log().insert(
                header_name as &str,
                Value::from(Bytes::from(value.to_owned())),
            );
        }
    }

    events
}

fn body_to_lines(buf: Bytes) -> impl Iterator<Item = Result<Bytes, ErrorMessage>> {
    let mut body = BytesMut::new();
    body.extend_from_slice(&buf);

    let mut decoder = BytesDelimitedCodec::new(b'\n');
    std::iter::from_fn(move || {
        match decoder.decode_eof(&mut body) {
            Err(e) => Some(Err(ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Bad request: {}", e),
            ))),
            Ok(Some(b)) => Some(Ok(b)),
            Ok(None) => None, // actually done
        }
    })
    .filter(|s| match s {
        // filter empty lines
        Ok(b) => !b.is_empty(),
        _ => true,
    })
}

fn decode_body(
    body: Bytes,
    enc: Encoding,
    metrics_plus_mode: bool,
) -> Result<Vec<Event>, ErrorMessage> {
    match enc {
        Encoding::Text => body_to_lines(body)
            .map(|r| Ok(Event::from(r?)))
            .collect::<Result<_, _>>(),
        Encoding::Ndjson => body_to_lines(body)
            .map(|j| {
                let parsed_json = serde_json::from_slice(&j?)
                    .map_err(|e| json_error(format!("Error parsing Ndjson: {:?}", e)))?;
                json_parse_object(parsed_json, metrics_plus_mode)
            })
            .collect::<Result<_, _>>(),
        Encoding::Json => {
            let parsed_json = serde_json::from_slice(&body)
                .map_err(|e| json_error(format!("Error parsing Json: {:?}", e)))?;
            json_parse_array_of_object(parsed_json, metrics_plus_mode)
        }
    }
}

fn json_parse_object(value: JsonValue, metrics_plus_mode: bool) -> Result<Event, ErrorMessage> {
    if metrics_plus_mode {
        let mut event = serde_json::from_value::<Event>(value);
        if let Ok(Event::Log(event)) = event.as_mut() {
            event.insert(log_schema().timestamp_key().clone(), Utc::now()); // Add timestamp
        }
        event.map_err(|e| json_error(format!("Unable to parse JSON as Event, {:?}", e)))
    } else {
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();
        log.insert(log_schema().timestamp_key().clone(), Utc::now()); // Add timestamp
        match value {
            JsonValue::Object(map) => {
                for (k, v) in map {
                    log.insert(k, v);
                }
                Ok(event)
            }
            _ => Err(json_error(format!(
                "Expected Object, got {}, the sending HTTP sink, if another vector instance, may have their metrics_plus_mode set to true, and mine is false.",
                json_value_to_type_string(&value)
            ))),
        }
    }
}

fn json_parse_array_of_object(
    value: JsonValue,
    metrics_plus_mode: bool,
) -> Result<Vec<Event>, ErrorMessage> {
    match value {
        JsonValue::Array(v) => v
            .into_iter()
            .map(|v| json_parse_object(v, metrics_plus_mode))
            .collect::<Result<_, _>>(),
        JsonValue::Object(map) => {
            //treat like an array of one object
            Ok(vec![json_parse_object(
                JsonValue::Object(map),
                metrics_plus_mode,
            )?])
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
        config::{log_schema, GlobalOptions, SourceConfig},
        event::Event,
        test_util::{collect_n, next_addr, trace_init, wait_for_tcp},
        Pipeline,
    };
    use futures::compat::Future01CompatExt;
    use futures01::sync::mpsc;
    use http::HeaderMap;
    use pretty_assertions::assert_eq;
    use std::net::SocketAddr;
    use string_cache::DefaultAtom as Atom;

    async fn source(
        encoding: Encoding,
        headers: Vec<String>,
    ) -> (mpsc::Receiver<Event>, SocketAddr) {
        source_metrics_plus(encoding, headers, false).await
    }

    async fn source_metrics_plus(
        encoding: Encoding,
        headers: Vec<String>,
        metrics_plus_mode: bool,
    ) -> (mpsc::Receiver<Event>, SocketAddr) {
        let (sender, recv) = Pipeline::new_test();
        let address = next_addr();
        tokio::spawn(async move {
            SimpleHttpConfig {
                address,
                encoding,
                headers,
                tls: None,
                metrics_plus_mode,
            }
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
            .await
            .unwrap()
            .compat()
            .await
            .unwrap();
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    async fn send(address: SocketAddr, body: &str) -> u16 {
        send_debug(address, body).await.status().as_u16()
    }

    async fn send_debug(address: SocketAddr, body: &str) -> reqwest::Response {
        reqwest::Client::new()
            .post(&format!("http://{}/", address))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
    }

    async fn send_with_headers(address: SocketAddr, body: &str, headers: HeaderMap) -> u16 {
        reqwest::Client::new()
            .post(&format!("http://{}/", address))
            .headers(headers)
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    #[tokio::test]
    async fn http_multiline_text() {
        trace_init();

        let body = "test body\n\ntest body 2";

        let (rx, addr) = source(Encoding::default(), vec![]).await;

        assert_eq!(200, send(addr, body).await);

        let mut events = collect_n(rx, 2).await.unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(
                log[&Atom::from(log_schema().message_key())],
                "test body".into()
            );
            assert!(log.get(&Atom::from(log_schema().timestamp_key())).is_some());
            assert_eq!(
                log[&Atom::from(log_schema().source_type_key())],
                "http".into()
            );
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(
                log[&Atom::from(log_schema().message_key())],
                "test body 2".into()
            );
            assert!(log.get(&Atom::from(log_schema().timestamp_key())).is_some());
            assert_eq!(
                log[&Atom::from(log_schema().source_type_key())],
                "http".into()
            );
        }
    }

    #[tokio::test]
    async fn http_multiline_text2() {
        trace_init();

        //same as above test but with a newline at the end
        let body = "test body\n\ntest body 2\n";

        let (rx, addr) = source(Encoding::default(), vec![]).await;

        assert_eq!(200, send(addr, body).await);

        let mut events = collect_n(rx, 2).await.unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(
                log[&Atom::from(log_schema().message_key())],
                "test body".into()
            );
            assert!(log.get(&Atom::from(log_schema().timestamp_key())).is_some());
            assert_eq!(
                log[&Atom::from(log_schema().source_type_key())],
                "http".into()
            );
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(
                log[&Atom::from(log_schema().message_key())],
                "test body 2".into()
            );
            assert!(log.get(&Atom::from(log_schema().timestamp_key())).is_some());
            assert_eq!(
                log[&Atom::from(log_schema().source_type_key())],
                "http".into()
            );
        }
    }

    #[tokio::test]
    async fn http_json_parsing() {
        trace_init();

        let (rx, addr) = source(Encoding::Json, vec![]).await;

        assert_eq!(400, send(addr, "{").await); //malformed
        assert_eq!(400, send(addr, r#"{"key"}"#).await); //key without value

        assert_eq!(200, send(addr, "{}").await); //can be one object or array of objects
        assert_eq!(200, send(addr, "[{},{},{}]").await);

        let mut events = collect_n(rx, 2).await.unwrap();
        assert!(events
            .remove(1)
            .as_log()
            .get(&Atom::from(log_schema().timestamp_key()))
            .is_some());
        assert!(events
            .remove(0)
            .as_log()
            .get(&Atom::from(log_schema().timestamp_key()))
            .is_some());
    }

    #[tokio::test]
    async fn http_json_parsing_metrics_plus_mode() {
        trace_init();

        let (rx, addr) = source_metrics_plus(Encoding::Json, vec![], true).await;

        assert_eq!(400, send(addr, "{").await); //malformed
        assert_eq!(400, send(addr, r#"{"key"}"#).await); //key without value
        assert_eq!(400, send(addr, r#"{}"#).await); //empty object should not be accepted in metrics_plus_mode

        assert_eq!(200, send(addr, "{\"Log\": {}}").await); //can be one object or array of objects
        assert_eq!(
            200,
            send(addr, "[{\"Log\": {}},{\"Log\": {}},{\"Log\": {}}]").await
        );

        let mut events = collect_n(rx, 2).await.unwrap();
        assert!(events
            .remove(1)
            .as_log()
            .get(&log_schema().timestamp_key())
            .is_some());
        assert!(events
            .remove(0)
            .as_log()
            .get(&log_schema().timestamp_key())
            .is_some());
    }

    #[tokio::test]
    async fn http_json_values() {
        trace_init();

        let (rx, addr) = source(Encoding::Json, vec![]).await;

        assert_eq!(200, send(addr, r#"[{"key":"value"}]"#).await);
        assert_eq!(200, send(addr, r#"{"key2":"value2"}"#).await);

        let mut events = collect_n(rx, 2).await.unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key")], "value".into());
            assert!(log.get(&Atom::from(log_schema().timestamp_key())).is_some());
            assert_eq!(
                log[&Atom::from(log_schema().source_type_key())],
                "http".into()
            );
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key2")], "value2".into());
            assert!(log.get(&Atom::from(log_schema().timestamp_key())).is_some());
            assert_eq!(
                log[&Atom::from(log_schema().source_type_key())],
                "http".into()
            );
        }
    }

    #[tokio::test]
    async fn http_json_values_metrics_plus_mode() {
        trace_init();

        let (rx, addr) = source_metrics_plus(Encoding::Json, vec![], true).await;

        assert_eq!(200, send(addr, r#"[{"Log":{"key":"value"}}]"#).await);
        assert_eq!(200, send(addr, r#"{"Log":{"key2":"value2"}}"#).await);

        let mut events = collect_n(rx, 2).await.unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key")], "value".into());
            assert!(log.get(&log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key2")], "value2".into());
            assert!(log.get(&log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
    }

    #[tokio::test]
    async fn http_ndjson() {
        trace_init();

        let (rx, addr) = source(Encoding::Ndjson, vec![]).await;

        assert_eq!(400, send(addr, r#"[{"key":"value"}]"#).await); //one object per line

        assert_eq!(
            200,
            send(addr, "{\"key1\":\"value1\"}\n\n{\"key2\":\"value2\"}").await
        );

        let mut events = collect_n(rx, 2).await.unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key1")], "value1".into());
            assert!(log.get(&Atom::from(log_schema().timestamp_key())).is_some());
            assert_eq!(
                log[&Atom::from(log_schema().source_type_key())],
                "http".into()
            );
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key2")], "value2".into());
            assert!(log.get(&Atom::from(log_schema().timestamp_key())).is_some());
            assert_eq!(
                log[&Atom::from(log_schema().source_type_key())],
                "http".into()
            );
        }
    }

    #[tokio::test]
    async fn http_ndjson_metrics_plus_mode() {
        trace_init();

        let (rx, addr) = source_metrics_plus(Encoding::Ndjson, vec![], true).await;

        assert_eq!(400, send(addr, r#"[{"Log":{"key":"value"}}]"#).await); //one object per line

        assert_eq!(
            200,
            send(
                addr,
                "{\"Log\":{\"key1\":\"value1\"}}\n\n{\"Log\":{\"key2\":\"value2\"}}"
            )
            .await
        );

        let mut events = collect_n(rx, 2).await.unwrap();
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key1")], "value1".into());
            assert!(log.get(&log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[&Atom::from("key2")], "value2".into());
            assert!(log.get(&log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
    }

    #[tokio::test]
    async fn http_headers() {
        trace_init();

        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", "test_client".parse().unwrap());
        headers.insert("Upgrade-Insecure-Requests", "false".parse().unwrap());

        let (rx, addr) = source(
            Encoding::Ndjson,
            vec![
                "User-Agent".to_string(),
                "Upgrade-Insecure-Requests".to_string(),
                "AbsentHeader".to_string(),
            ],
        )
        .await;

        assert_eq!(
            200,
            send_with_headers(addr, "{\"key1\":\"value1\"}", headers).await
        );

        let mut events = collect_n(rx, 1).await.unwrap();
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
            assert!(log.get(&Atom::from(log_schema().timestamp_key())).is_some());
            assert_eq!(
                log[&Atom::from(log_schema().source_type_key())],
                "http".into()
            );
        }
    }
}
