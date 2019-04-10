use super::util::{
    self, retries::FixedRetryPolicy, BatchServiceSink, Buffer, Compression, SinkExt,
};
use crate::buffers::Acker;
use crate::bytes::BytesExt;
use crate::record::Record;
use futures::{Future, Sink};
use http::{Method, Uri};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use string_cache::DefaultAtom as Atom;
use tower::ServiceBuilder;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct HecSinkConfig {
    pub token: String,
    pub host: String,
    pub buffer_size: Option<usize>,
    pub compression: Option<Compression>,
    pub request_timeout_secs: Option<u64>,
    pub retries: Option<usize>,
    pub in_flight_request_limit: Option<usize>,
    #[serde(default = "default_host_field")]
    pub host_field: Atom,
}

fn default_host_field() -> Atom {
    "host".into()
}

#[typetag::serde(name = "splunk_hec")]
impl crate::topology::config::SinkConfig for HecSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((
            hec(self.clone(), acker),
            hec_healthcheck(self.token.clone(), self.host.clone()),
        ))
    }
}

pub fn hec(config: HecSinkConfig, acker: Acker) -> super::RouterSink {
    let host = config.host.clone();
    let token = config.token.clone();
    let buffer_size = config.buffer_size.unwrap_or(2 * 1024 * 1024);
    let gzip = match config.compression.unwrap_or(Compression::Gzip) {
        Compression::None => false,
        Compression::Gzip => true,
    };
    let timeout_secs = config.request_timeout_secs.unwrap_or(10);
    let retries = config.retries.unwrap_or(5);
    let in_flight_limit = config.in_flight_request_limit.unwrap_or(1);
    let host_field = config.host_field;

    let policy = FixedRetryPolicy::new(retries, Duration::from_secs(1), util::http::HttpRetryLogic);

    let http_service = util::http::HttpService::new(move |body: Vec<u8>| {
        let uri = format!("{}/services/collector/event", host);
        let uri: Uri = uri.parse().unwrap();

        let mut builder = hyper::Request::builder();
        builder.method(Method::POST);
        builder.uri(uri);

        builder.header("Content-Type", "application/json");
        builder.header("Content-Encoding", "gzip");
        builder.header("Authorization", format!("Splunk {}", token));

        builder.body(body.into()).unwrap()
    });
    let service = ServiceBuilder::new()
        .retry(policy)
        .in_flight_limit(in_flight_limit)
        .timeout(Duration::from_secs(timeout_secs))
        .service(http_service)
        .expect("This is a bug, no spawning");

    let sink = BatchServiceSink::new(service, acker)
        .batched(Buffer::new(gzip), buffer_size)
        .with(move |record: Record| {
            let host = record.structured.get(&host_field).map(|h| h.clone());

            let mut body = json!({
                "event": String::from_utf8_lossy(&record.raw[..]),
                "fields": record.structured
                    .into_iter()
                    .map(|(k, v)| (k, v.as_utf8_lossy().into_owned()))
                    .collect::<HashMap<Atom, String>>(),
            });

            if let Some(host) = host {
                let host = host.as_utf8_lossy();
                body["host"] = json!(host);
            }
            let body = serde_json::to_vec(&body).unwrap();
            Ok(body)
        });

    Box::new(sink)
}

pub fn hec_healthcheck(token: String, host: String) -> super::Healthcheck {
    use hyper::{Body, Client, Request};
    use hyper_tls::HttpsConnector;

    let uri = format!("{}/services/collector/health/1.0", host);
    let uri: Uri = uri.parse().unwrap();

    let request = Request::get(uri)
        .header("Authorization", format!("Splunk {}", token))
        .body(Body::empty())
        .unwrap();

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build(https);

    let healthcheck = client
        .request(request)
        .map_err(|err| err.to_string())
        .and_then(|response| {
            use hyper::StatusCode;

            match response.status() {
                StatusCode::OK => Ok(()),
                StatusCode::BAD_REQUEST => Err("Invalid HEC token".to_string()),
                StatusCode::SERVICE_UNAVAILABLE => {
                    Err("HEC is unhealthy, queues are full".to_string())
                }
                other => Err(format!("Unexpected status: {}", other)),
            }
        });

    Box::new(healthcheck)
}

#[cfg(test)]
mod tests {
    #![cfg(feature = "splunk-integration-tests")]

    use crate::buffers::Acker;
    use crate::{
        sinks,
        test_util::{random_lines_with_stream, random_string},
        Record,
    };
    use futures::Sink;
    use serde_json::Value as JsonValue;

    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    #[test]
    fn splunk_insert_message() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk::hec(config(), Acker::Null);

        let message = random_string(100);
        let record = Record::from(message.clone());

        let pump = sink.send(record);

        rt.block_on(pump).unwrap();

        // It usually takes ~1 second for the event to show up in search, so poll until
        // we see it.
        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap() == message)
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["_raw"].as_str().unwrap());
    }

    #[test]
    fn splunk_insert_many() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk::hec(config(), Acker::Null);

        let (messages, records) = random_lines_with_stream(100, 10);

        let pump = sink.send_all(records);

        rt.block_on(pump).unwrap();

        let mut found_all = false;
        for _ in 0..20 {
            let entries = recent_entries();

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
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk::hec(config(), Acker::Null);

        let message = random_string(100);
        let mut record = Record::from(message.clone());
        record.structured.insert("asdf".into(), "hello".into());

        let pump = sink.send(record);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap() == message)
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["_raw"].as_str().unwrap());
        assert_eq!("hello", entry["asdf"].as_str().unwrap());
    }

    #[test]
    fn splunk_hostname() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk::hec(config(), Acker::Null);

        let message = random_string(100);
        let mut record = Record::from(message.clone());
        record.structured.insert("asdf".into(), "hello".into());
        record
            .structured
            .insert("host".into(), "example.com:1234".into());

        let pump = sink.send(record);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap() == message)
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["_raw"].as_str().unwrap());
        assert_eq!("hello", entry["asdf"].as_str().unwrap());
        assert_eq!("example.com:1234", entry["host"].as_str().unwrap());
    }

    #[test]
    fn splunk_configure_hostname() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let config = super::HecSinkConfig {
            host_field: "roast".into(),
            ..config()
        };

        let sink = sinks::splunk::hec(config, Acker::Null);

        let message = random_string(100);
        let mut record = Record::from(message.clone());
        record.structured.insert("asdf".into(), "hello".into());
        record
            .structured
            .insert("host".into(), "example.com:1234".into());
        record
            .structured
            .insert("roast".into(), "beef.example.com:1234".into());

        let pump = sink.send(record);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap() == message)
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["_raw"].as_str().unwrap());
        assert_eq!("hello", entry["asdf"].as_str().unwrap());
        assert_eq!("beef.example.com:1234", entry["host"].as_str().unwrap());
    }

    #[test]
    fn splunk_healthcheck() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        // OK
        {
            let healthcheck =
                sinks::splunk::hec_healthcheck(get_token(), "http://localhost:8088".to_string());
            rt.block_on(healthcheck).unwrap();
        }

        // Server not listening at address
        {
            let healthcheck =
                sinks::splunk::hec_healthcheck(get_token(), "http://localhost:1111".to_string());

            let err = rt.block_on(healthcheck).unwrap_err();
            assert!(err.starts_with("an error occurred trying to connect"));
        }

        // Invalid token
        // The HEC REST docs claim that the healthcheck endpoint will validate the auth token,
        // but my local testing server returns 200 even with a bad token.
        {
            // let healthcheck = sinks::splunk::hec_healthcheck("asdf".to_string(), "http://localhost:8088".to_string());
            // assert_eq!(rt.block_on(healthcheck).unwrap_err(), "Invalid HEC token");
        }

        // Unhealthy server
        {
            let healthcheck =
                sinks::splunk::hec_healthcheck(get_token(), "http://503.returnco.de".to_string());
            assert_eq!(
                rt.block_on(healthcheck).unwrap_err(),
                "HEC is unhealthy, queues are full"
            );
        }
    }

    fn recent_entries() -> Vec<JsonValue> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        // http://docs.splunk.com/Documentation/Splunk/7.2.1/RESTREF/RESTsearch#search.2Fjobs
        let mut res = client
            .post("https://localhost:8089/services/search/jobs?output_mode=json")
            .form(&[
                ("search", "search *"),
                ("exec_mode", "oneshot"),
                ("f", "_raw"),
                ("f", "asdf"),
                ("f", "host"),
            ])
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .unwrap();
        let json: JsonValue = res.json().unwrap();

        json["results"].as_array().unwrap().clone()
    }

    fn config() -> super::HecSinkConfig {
        super::HecSinkConfig {
            host: "http://localhost:8088/".into(),
            token: get_token(),
            buffer_size: None,
            compression: None,
            request_timeout_secs: None,
            retries: None,
            in_flight_request_limit: None,
            host_field: "host".into(),
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

        let token = entries[0]["content"]["token"].as_str().unwrap().to_owned();

        token
    }
}
