use crate::{
    config::{
        log_schema, DataType, GenerateConfig, GlobalOptions, Resource, SourceConfig,
        SourceDescription,
    },
    event::{Event, Value},
    shutdown::ShutdownSignal,
    sources::util::{add_query_parameters, ErrorMessage, HttpSource, HttpSourceAuthConfig},
    tls::TlsConfig,
    Pipeline,
};
use bytes::{Bytes, BytesMut};
use chrono::Utc;
use codec::BytesDelimitedCodec;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::{collections::HashMap, net::SocketAddr};

use tokio_util::codec::Decoder;
use warp::http::{HeaderMap, HeaderValue, StatusCode};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SimpleHttpConfig {
    address: SocketAddr,
    #[serde(default)]
    encoding: Encoding,
    #[serde(default)]
    headers: Vec<String>,
    #[serde(default)]
    query_parameters: Vec<String>,
    tls: Option<TlsConfig>,
    auth: Option<HttpSourceAuthConfig>,
}

inventory::submit! {
    SourceDescription::new::<SimpleHttpConfig>("http")
}

impl GenerateConfig for SimpleHttpConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:80".parse().unwrap(),
            encoding: Default::default(),
            headers: Vec::new(),
            query_parameters: Vec::new(),
            tls: None,
            auth: None,
        })
        .unwrap()
    }
}

#[derive(Clone)]
struct SimpleHttpSource {
    encoding: Encoding,
    headers: Vec<String>,
    query_parameters: Vec<String>,
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
    fn build_event(
        &self,
        body: Bytes,
        header_map: HeaderMap,
        query_parameters: HashMap<String, String>,
    ) -> Result<Vec<Event>, ErrorMessage> {
        decode_body(body, self.encoding)
            .map(|events| add_headers(events, &self.headers, header_map))
            .map(|events| add_query_parameters(events, &self.query_parameters, query_parameters))
            .map(|mut events| {
                // Add source type
                let key = log_schema().source_type_key();
                for event in events.iter_mut() {
                    event.as_mut_log().try_insert(key, Bytes::from("http"));
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
        let source = SimpleHttpSource {
            encoding: self.encoding,
            headers: self.headers.clone(),
            query_parameters: self.query_parameters.clone(),
        };
        source.run(self.address, "", &self.tls, &self.auth, out, shutdown)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "http"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.into()]
    }
}

fn add_headers(
    mut events: Vec<Event>,
    headers_config: &[String],
    headers: HeaderMap,
) -> Vec<Event> {
    for header_name in headers_config {
        let value = headers.get(header_name).map(HeaderValue::as_bytes);

        for event in events.iter_mut() {
            event.as_mut_log().insert(
                header_name as &str,
                Value::from(value.map(Bytes::copy_from_slice)),
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
            Err(error) => Some(Err(ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Bad request: {}", error),
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

fn decode_body(body: Bytes, enc: Encoding) -> Result<Vec<Event>, ErrorMessage> {
    match enc {
        Encoding::Text => body_to_lines(body)
            .map(|r| Ok(Event::from(r?)))
            .collect::<Result<_, _>>(),
        Encoding::Ndjson => body_to_lines(body)
            .map(|j| {
                let parsed_json = serde_json::from_slice(&j?)
                    .map_err(|error| json_error(format!("Error parsing Ndjson: {:?}", error)))?;
                json_parse_object(parsed_json)
            })
            .collect::<Result<_, _>>(),
        Encoding::Json => {
            let parsed_json = serde_json::from_slice(&body)
                .map_err(|error| json_error(format!("Error parsing Json: {:?}", error)))?;
            json_parse_array_of_object(parsed_json)
        }
    }
}

fn json_parse_object(value: JsonValue) -> Result<Event, ErrorMessage> {
    let mut event = Event::new_empty_log();
    let log = event.as_mut_log();
    log.insert(log_schema().timestamp_key(), Utc::now()); // Add timestamp
    match value {
        JsonValue::Object(map) => {
            for (k, v) in map {
                log.insert_flat(k, v);
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
        config::{log_schema, GlobalOptions, SourceConfig},
        event::{Event, Value},
        test_util::{collect_n, next_addr, trace_init, wait_for_tcp},
        Pipeline,
    };
    use http::HeaderMap;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use std::net::SocketAddr;
    use tokio::sync::mpsc;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SimpleHttpConfig>();
    }

    async fn source(
        encoding: Encoding,
        headers: Vec<String>,
        query_parameters: Vec<String>,
    ) -> (mpsc::Receiver<Event>, SocketAddr) {
        let (sender, recv) = Pipeline::new_test();
        let address = next_addr();
        tokio::spawn(async move {
            SimpleHttpConfig {
                address,
                encoding,
                headers,
                query_parameters,
                tls: None,
                auth: None,
            }
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
            .await
            .unwrap()
            .await
            .unwrap();
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    async fn send(address: SocketAddr, body: &str) -> u16 {
        reqwest::Client::new()
            .post(&format!("http://{}/", address))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
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

    async fn send_with_query(address: SocketAddr, body: &str, query: &str) -> u16 {
        reqwest::Client::new()
            .post(&format!("http://{}?{}", address, query))
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

        let (rx, addr) = source(Encoding::default(), vec![], vec![]).await;

        assert_eq!(200, send(addr, body).await);

        let mut events = collect_n(rx, 2).await;
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "test body".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "test body 2".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
    }

    #[tokio::test]
    async fn http_multiline_text2() {
        trace_init();

        //same as above test but with a newline at the end
        let body = "test body\n\ntest body 2\n";

        let (rx, addr) = source(Encoding::default(), vec![], vec![]).await;

        assert_eq!(200, send(addr, body).await);

        let mut events = collect_n(rx, 2).await;
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "test body".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "test body 2".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
    }

    #[tokio::test]
    async fn http_json_parsing() {
        trace_init();

        let (rx, addr) = source(Encoding::Json, vec![], vec![]).await;

        assert_eq!(400, send(addr, "{").await); //malformed
        assert_eq!(400, send(addr, r#"{"key"}"#).await); //key without value

        assert_eq!(200, send(addr, "{}").await); //can be one object or array of objects
        assert_eq!(200, send(addr, "[{},{},{}]").await);

        let mut events = collect_n(rx, 2).await;
        assert!(events
            .remove(1)
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
        assert!(events
            .remove(0)
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
    }

    #[tokio::test]
    async fn http_json_values() {
        trace_init();

        let (rx, addr) = source(Encoding::Json, vec![], vec![]).await;

        assert_eq!(200, send(addr, r#"[{"key":"value"}]"#).await);
        assert_eq!(200, send(addr, r#"{"key2":"value2"}"#).await);

        let mut events = collect_n(rx, 2).await;
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key"], "value".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key2"], "value2".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
    }

    #[tokio::test]
    async fn http_json_dotted_keys() {
        trace_init();

        let (rx, addr) = source(Encoding::Json, vec![], vec![]).await;

        assert_eq!(200, send(addr, r#"[{"dotted.key":"value"}]"#).await);
        assert_eq!(
            200,
            send(addr, r#"{"nested":{"dotted.key2":"value2"}}"#).await
        );

        let mut events = collect_n(rx, 2).await;
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log.get_flat("dotted.key").unwrap(), &Value::from("value"));
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            let mut map = BTreeMap::new();
            map.insert("dotted.key2".to_string(), Value::from("value2"));
            assert_eq!(log["nested"], map.into());
        }
    }

    #[tokio::test]
    async fn http_ndjson() {
        trace_init();

        let (rx, addr) = source(Encoding::Ndjson, vec![], vec![]).await;

        assert_eq!(400, send(addr, r#"[{"key":"value"}]"#).await); //one object per line

        assert_eq!(
            200,
            send(addr, "{\"key1\":\"value1\"}\n\n{\"key2\":\"value2\"}").await
        );

        let mut events = collect_n(rx, 2).await;
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key2"], "value2".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
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
            vec![],
        )
        .await;

        assert_eq!(
            200,
            send_with_headers(addr, "{\"key1\":\"value1\"}", headers).await
        );

        let mut events = collect_n(rx, 1).await;
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert_eq!(log["User-Agent"], "test_client".into());
            assert_eq!(log["Upgrade-Insecure-Requests"], "false".into());
            assert_eq!(log["AbsentHeader"], Value::Null);
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
    }

    #[tokio::test]
    async fn http_query() {
        trace_init();
        let (rx, addr) = source(
            Encoding::Ndjson,
            vec![],
            vec![
                "source".to_string(),
                "region".to_string(),
                "absent".to_string(),
            ],
        )
        .await;

        assert_eq!(
            200,
            send_with_query(addr, "{\"key1\":\"value1\"}", "source=staging&region=gb").await
        );

        let mut events = collect_n(rx, 1).await;
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert_eq!(log["source"], "staging".into());
            assert_eq!(log["region"], "gb".into());
            assert_eq!(log["absent"], Value::Null);
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(log[log_schema().source_type_key()], "http".into());
        }
    }
}
