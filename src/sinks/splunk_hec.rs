use crate::{
    dns::Resolver,
    event::{self, Event, LogEvent, Value},
    internal_events::{SplunkEventEncodeError, SplunkEventSent},
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::{BatchedHttpSink, HttpClient, HttpSink},
        service2::TowerRequestConfig,
        BatchBytesConfig, Buffer, Compression,
    },
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::{FutureExt, TryFutureExt};
use futures01::Sink;
use http::{Request, StatusCode, Uri};
use hyper::Body;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::{ResultExt, Snafu};
use std::convert::TryFrom;
use string_cache::DefaultAtom as Atom;

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("Host must include a scheme (https:// or http://)"))]
    UriMissingScheme,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct HecSinkConfig {
    pub token: String,
    pub host: String,
    #[serde(default = "default_host_key")]
    pub host_key: Atom,
    #[serde(default)]
    pub indexed_fields: Vec<Atom>,
    pub index: Option<String>,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        in_flight_limit: Some(10),
        rate_limit_num: Some(10),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Json,
}

fn default_host_key() -> Atom {
    event::LogSchema::default().host_key().clone()
}

inventory::submit! {
    SinkDescription::new::<HecSinkConfig>("splunk_hec")
}

#[typetag::serde(name = "splunk_hec")]
impl SinkConfig for HecSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        validate_host(&self.host)?;

        let batch = self.batch.unwrap_or(bytesize::mib(1u64), 1);
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);
        let tls_settings = TlsSettings::from_options(&self.tls)?;

        let sink = BatchedHttpSink::new(
            self.clone(),
            Buffer::new(self.compression),
            request,
            batch,
            tls_settings,
            &cx,
        )
        .sink_map_err(|e| error!("Fatal splunk_hec sink error: {}", e));

        let healthcheck = healthcheck(self.clone(), cx.resolver()).boxed().compat();

        Ok((Box::new(sink), Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "splunk_hec"
    }
}

#[async_trait::async_trait]
impl HttpSink for HecSinkConfig {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);

        let mut event = event.into_log();

        let host = event.get(&self.host_key).cloned();

        let timestamp = match event.remove(&event::log_schema().timestamp_key()) {
            Some(Value::Timestamp(ts)) => ts,
            _ => chrono::Utc::now(),
        };
        let timestamp = (timestamp.timestamp_millis() as f64) / 1000f64;

        let sourcetype = event.get(&event::log_schema().source_type_key()).cloned();

        let fields = self
            .indexed_fields
            .iter()
            .filter_map(|field| event.get(field).map(|value| (field, value.clone())))
            .collect::<LogEvent>();

        let event = match self.encoding.codec() {
            Encoding::Json => json!(event),
            Encoding::Text => json!(event
                .get(&event::log_schema().message_key())
                .map(|v| v.to_string_lossy())
                .unwrap_or_else(|| "".into())),
        };

        let mut body = json!({
            "event": event,
            "fields": fields,
            "time": timestamp
        });

        if let Some(host) = host {
            let host = host.to_string_lossy();
            body["host"] = json!(host);
        }

        if let Some(index) = &self.index {
            body["index"] = json!(index);
        }

        if let Some(sourcetype) = sourcetype {
            let sourcetype = sourcetype.to_string_lossy();
            body["sourcetype"] = json!(sourcetype);
        }

        match serde_json::to_vec(&body) {
            Ok(value) => {
                emit!(SplunkEventSent {
                    byte_size: value.len()
                });
                Some(value)
            }
            Err(e) => {
                emit!(SplunkEventEncodeError { error: e });
                None
            }
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        let uri = build_uri(&self.host, "/services/collector/event").expect("Unable to parse URI");

        let mut builder = Request::post(uri)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Splunk {}", self.token));

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        builder.body(events).map_err(Into::into)
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Invalid HEC token"))]
    InvalidToken,
    #[snafu(display("Queues are full"))]
    QueuesFull,
}

pub async fn healthcheck(config: HecSinkConfig, resolver: Resolver) -> crate::Result<()> {
    let uri =
        build_uri(&config.host, "/services/collector/health/1.0").context(super::UriParseError)?;

    let request = Request::get(uri)
        .header("Authorization", format!("Splunk {}", config.token))
        .body(Body::empty())
        .unwrap();

    let tls = TlsSettings::from_options(&config.tls)?;
    let mut client = HttpClient::new(resolver, tls)?;

    let response = client.send(request).await?;
    match response.status() {
        StatusCode::OK => Ok(()),
        StatusCode::BAD_REQUEST => Err(HealthcheckError::InvalidToken.into()),
        StatusCode::SERVICE_UNAVAILABLE => Err(HealthcheckError::QueuesFull.into()),
        other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

pub fn validate_host(host: &str) -> crate::Result<()> {
    let uri = Uri::try_from(host).context(super::UriParseError)?;

    match uri.scheme() {
        Some(_) => Ok(()),
        None => Err(Box::new(BuildError::UriMissingScheme)),
    }
}

fn build_uri(host: &str, path: &str) -> Result<Uri, http::uri::InvalidUri> {
    format!("{}{}", host.trim_end_matches('/'), path).parse::<Uri>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use crate::sinks::util::{http::HttpSink, test::load_sink};
    use chrono::Utc;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Deserialize, Debug)]
    struct HecEventJson {
        time: f64,
        event: BTreeMap<String, String>,
        fields: BTreeMap<String, String>,
    }

    #[derive(Deserialize, Debug)]
    struct HecEventText {
        time: f64,
        event: String,
        fields: BTreeMap<String, String>,
    }

    #[test]
    fn splunk_encode_event_json() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("key", "value");

        let (config, _, _) = load_sink::<HecSinkConfig>(
            r#"
            host = "test.com"
            token = "alksjdfo"
            host_key = "host"
            indexed_fields = ["key"]

            [encoding]
            codec = "json"
        "#,
        )
        .unwrap();

        let bytes = config.encode_event(event).unwrap();

        let hec_event = serde_json::from_slice::<HecEventJson>(&bytes[..]).unwrap();

        let event = &hec_event.event;
        let kv = event.get(&"key".to_string()).unwrap();

        assert_eq!(kv, &"value".to_string());
        assert_eq!(
            event[&event::log_schema().message_key().to_string()],
            "hello world".to_string()
        );
        assert!(event
            .get(&event::log_schema().timestamp_key().to_string())
            .is_none());

        assert_eq!(
            hec_event.fields.get("key").map(|s| s.as_str()),
            Some("value")
        );

        let now = Utc::now().timestamp_millis() as f64 / 1000f64;
        assert!(
            (hec_event.time - now).abs() < 0.2,
            format!("hec_event.time = {}, now = {}", hec_event.time, now)
        );
        assert_eq!((hec_event.time * 1000f64).fract(), 0f64);
    }

    #[test]
    fn splunk_encode_event_text() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("key", "value");

        let (config, _, _) = load_sink::<HecSinkConfig>(
            r#"
            host = "test.com"
            token = "alksjdfo"
            host_key = "host"
            indexed_fields = ["key"]

            [encoding]
            codec = "text"
        "#,
        )
        .unwrap();

        let bytes = config.encode_event(event).unwrap();

        let hec_event = serde_json::from_slice::<HecEventText>(&bytes[..]).unwrap();

        assert_eq!(hec_event.event.as_str(), "hello world");

        assert_eq!(
            hec_event.fields.get("key").map(|s| s.as_str()),
            Some("value")
        );

        let now = Utc::now().timestamp_millis() as f64 / 1000f64;
        assert!(
            (hec_event.time - now).abs() < 0.2,
            format!("hec_event.time = {}, now = {}", hec_event.time, now)
        );
        assert_eq!((hec_event.time * 1000f64).fract(), 0f64);
    }

    #[test]
    fn splunk_validate_host() {
        let valid = "http://localhost:8888".to_string();
        let invalid_scheme = "localhost:8888".to_string();
        let invalid_uri = "iminvalidohnoes".to_string();

        assert!(validate_host(&valid).is_ok());
        assert!(validate_host(&invalid_scheme).is_err());
        assert!(validate_host(&invalid_uri).is_err());
    }

    #[test]
    fn splunk_build_uri() {
        let uri = build_uri("http://test.com/", "/a");

        assert!(uri.is_ok());
        assert_eq!(format!("{}", uri.unwrap()), "http://test.com/a");
    }
}

#[cfg(test)]
#[cfg(feature = "splunk-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        assert_downcast_matches, sinks,
        test_util::{random_lines_with_stream, random_string, runtime},
        topology::config::{SinkConfig, SinkContext},
        Event,
    };
    use futures01::Sink;
    use serde_json::Value as JsonValue;
    use std::net::SocketAddr;
    use warp::Filter;

    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    #[test]
    fn splunk_insert_message() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let config = config(Encoding::Text, vec![]);
        let (sink, _) = config.build(cx).unwrap();

        let message = random_string(100);
        let event = Event::from(message.clone());

        let pump = sink.send(event);

        rt.block_on(pump).unwrap();

        // It usually takes ~1 second for the event to show up in search, so poll until
        // we see it.
        let entry = (0..20)
            .find_map(|_| {
                recent_entries(None)
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap() == message)
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["_raw"].as_str().unwrap());
        assert!(entry.get("message").is_none());
    }

    #[test]
    fn splunk_insert_index() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let mut config = config(Encoding::Text, vec![]);
        config.index = Some("custom_index".to_string());
        let (sink, _) = config.build(cx).unwrap();

        let message = random_string(100);
        let event = Event::from(message.clone());

        let pump = sink.send(event);

        rt.block_on(pump).unwrap();

        // It usually takes ~1 second for the event to show up in search, so poll until
        // we see it.
        let entry = (0..20)
            .find_map(|_| {
                recent_entries(Some("custom_index"))
                    .into_iter()
                    .find(|entry| entry["index"].as_str().unwrap() == "custom_index")
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(entry["index"].as_str().unwrap(), "custom_index");
    }

    #[test]
    fn splunk_insert_many() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let config = config(Encoding::Text, vec![]);
        let (sink, _) = config.build(cx).unwrap();

        let (messages, events) = random_lines_with_stream(100, 10);

        let pump = sink.send_all(events);

        let _ = rt.block_on(pump).unwrap();

        let mut found_all = false;
        for _ in 0..20 {
            let entries = recent_entries(None);

            found_all = messages.iter().all(|message| {
                entries
                    .iter()
                    .any(|entry| entry["_raw"].as_str().unwrap() == message)
            });

            if found_all {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        assert!(found_all);
    }

    #[test]
    fn splunk_custom_fields() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let indexed_fields = vec![Atom::from("asdf")];
        let config = config(Encoding::Json, indexed_fields);
        let (sink, _) = config.build(cx).unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("asdf", "hello");

        let pump = sink.send(event);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries(None)
                    .into_iter()
                    .find(|entry| entry["message"].as_str() == Some(message.as_str()))
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["message"].as_str().unwrap());
        let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("hello", asdf);
    }

    #[test]
    fn splunk_hostname() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let indexed_fields = vec![Atom::from("asdf")];
        let config = config(Encoding::Json, indexed_fields);
        let (sink, _) = config.build(cx).unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("asdf", "hello");
        event.as_mut_log().insert("host", "example.com:1234");

        let pump = sink.send(event);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries(None)
                    .into_iter()
                    .find(|entry| entry["message"].as_str() == Some(message.as_str()))
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["message"].as_str().unwrap());
        let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("hello", asdf);
        let host = entry["host"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("example.com:1234", host);
    }

    #[test]
    fn splunk_sourcetype() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let indexed_fields = vec![Atom::from("asdf")];
        let config = config(Encoding::Json, indexed_fields);
        let (sink, _) = config.build(cx).unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("asdf", "hello");
        event
            .as_mut_log()
            .insert(event::log_schema().source_type_key(), "file");

        let pump = sink.send(event);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries(None)
                    .into_iter()
                    .find(|entry| entry["message"].as_str() == Some(message.as_str()))
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["message"].as_str().unwrap());
        let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("hello", asdf);
        let sourcetype = entry["sourcetype"].as_str().unwrap();
        assert_eq!("file", sourcetype);
    }

    #[test]
    fn splunk_configure_hostname() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let config = super::HecSinkConfig {
            host_key: "roast".into(),
            ..config(Encoding::Json, vec![Atom::from("asdf")])
        };

        let (sink, _) = config.build(cx).unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("asdf", "hello");
        event.as_mut_log().insert("host", "example.com:1234");
        event.as_mut_log().insert("roast", "beef.example.com:1234");

        let pump = sink.send(event);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries(None)
                    .into_iter()
                    .find(|entry| entry["message"].as_str() == Some(message.as_str()))
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["message"].as_str().unwrap());
        let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("hello", asdf);
        let host = entry["host"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("beef.example.com:1234", host);
    }

    #[test]
    fn splunk_healthcheck() {
        let mut rt = runtime();
        let resolver = crate::dns::Resolver;

        // OK
        {
            let config = config(Encoding::Text, vec![]);
            let healthcheck = sinks::splunk_hec::healthcheck(config, resolver.clone());
            rt.block_on_std(healthcheck).unwrap();
        }

        // Server not listening at address
        {
            let config = HecSinkConfig {
                host: "http://localhost:1111".to_string(),
                ..config(Encoding::Text, vec![])
            };
            let healthcheck = sinks::splunk_hec::healthcheck(config, resolver.clone());

            rt.block_on_std(healthcheck).unwrap_err();
        }

        // Invalid token
        // The HEC REST docs claim that the healthcheck endpoint will validate the auth token,
        // but my local testing server returns 200 even with a bad token.
        // {
        //     let healthcheck = sinks::splunk::healthcheck(
        //         "wrong".to_string(),
        //         "http://localhost:8088".to_string(),
        //     )
        //     .unwrap();

        //     assert_eq!(rt.block_on(healthcheck).unwrap_err(), "Invalid HEC token");
        // }

        // Unhealthy server
        {
            let config = HecSinkConfig {
                host: "http://localhost:5503".to_string(),
                ..config(Encoding::Text, vec![])
            };

            let unhealthy = warp::any()
                .map(|| warp::reply::with_status("i'm sad", StatusCode::SERVICE_UNAVAILABLE));
            let server = warp::serve(unhealthy).bind("0.0.0.0:5503".parse::<SocketAddr>().unwrap());
            rt.spawn_std(server);

            let healthcheck = sinks::splunk_hec::healthcheck(config, resolver);
            assert_downcast_matches!(
                rt.block_on_std(healthcheck).unwrap_err(),
                HealthcheckError,
                HealthcheckError::QueuesFull
            );
        }
    }

    fn recent_entries(index: Option<&str>) -> Vec<JsonValue> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        // https://docs.splunk.com/Documentation/Splunk/7.2.1/RESTREF/RESTsearch#search.2Fjobs
        let search_query = match index {
            Some(index) => format!("search index={}", index),
            None => "search *".into(),
        };
        let mut res = client
            .post("https://localhost:8089/services/search/jobs?output_mode=json")
            .form(&vec![
                ("search", &search_query[..]),
                ("exec_mode", "oneshot"),
                ("f", "*"),
            ])
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .unwrap();
        let json: JsonValue = res.json().unwrap();

        println!("output: {:?}", json);

        json["results"].as_array().unwrap().clone()
    }

    fn config(
        encoding: impl Into<EncodingConfigWithDefault<Encoding>>,
        indexed_fields: Vec<Atom>,
    ) -> super::HecSinkConfig {
        super::HecSinkConfig {
            host: "http://localhost:8088/".into(),
            token: get_token(),
            host_key: "host".into(),
            compression: Compression::None,
            encoding: encoding.into(),
            batch: BatchBytesConfig {
                max_size: Some(1),
                timeout_secs: None,
            },
            indexed_fields,
            ..Default::default()
        }
    }

    fn get_token() -> String {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let mut res = client
            .get("https://localhost:8089/services/data/inputs/http?output_mode=json")
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .unwrap();

        let json: JsonValue = res.json().unwrap();
        let entries = json["entry"].as_array().unwrap().clone();

        if entries.is_empty() {
            // TODO: create one automatically
            panic!("You don't have any HTTP Event Collector inputs set up in Splunk");
        }

        entries[0]["content"]["token"].as_str().unwrap().to_owned()
    }
}
