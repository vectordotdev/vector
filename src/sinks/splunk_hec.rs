use crate::{
    buffers::Acker,
    event::{self, Event, ValueKind},
    sinks::util::{
        http::{HttpRetryLogic, HttpService},
        retries::FixedRetryPolicy,
        BatchServiceSink, Buffer, Compression, SinkExt,
    },
};
use bytes::Bytes;
use futures::{Future, Sink};
use http::{HttpTryFrom, Method, Request, StatusCode, Uri};
use hyper::{Body, Client};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use string_cache::DefaultAtom as Atom;
use tower::ServiceBuilder;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct HecSinkConfig {
    pub token: String,
    pub host: String,
    #[serde(default = "default_host_field")]
    pub host_field: Atom,
    pub batch_size: Option<usize>,
    pub batch_timeout: Option<u64>,
    pub compression: Option<Compression>,
    pub encoding: Option<Encoding>,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

fn default_host_field() -> Atom {
    event::HOST.clone()
}

#[typetag::serde(name = "splunk_hec")]
impl crate::topology::config::SinkConfig for HecSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        validate_host(&self.host)?;
        let sink = hec(self.clone(), acker)?;
        let healtcheck = healthcheck(self.token.clone(), self.host.clone())?;

        Ok((sink, healtcheck))
    }
}

pub fn hec(config: HecSinkConfig, acker: Acker) -> Result<super::RouterSink, String> {
    let host = config.host.clone();
    let token = config.token.clone();
    let host_field = config.host_field;

    let batch_size = config.batch_size.unwrap_or(2 * 1024 * 1024);
    let gzip = match config.compression.unwrap_or(Compression::Gzip) {
        Compression::None => false,
        Compression::Gzip => true,
    };
    let batch_timeout = config.batch_timeout.unwrap_or(300);

    let timeout = config.request_timeout_secs.unwrap_or(10);
    let in_flight_limit = config.request_in_flight_limit.unwrap_or(1);
    let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
    let rate_limit_num = config.request_rate_limit_num.unwrap_or(10);
    let retry_attempts = config.request_retry_attempts.unwrap_or(5);
    let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);
    let encoding = config.encoding.clone();

    let policy = FixedRetryPolicy::new(
        retry_attempts,
        Duration::from_secs(retry_backoff_secs),
        HttpRetryLogic,
    );

    let uri = format!("{}/services/collector/event", host)
        .parse::<Uri>()
        .map_err(|e| format!("{}", e))?;
    let token = Bytes::from(format!("Splunk {}", token));

    let http_service = HttpService::new(move |body: Vec<u8>| {
        let mut builder = Request::builder();
        builder.method(Method::POST);
        builder.uri(uri.clone());

        builder.header("Content-Type", "application/json");

        if gzip {
            builder.header("Content-Encoding", "gzip");
        }

        builder.header("Authorization", token.clone());

        builder.body(body).unwrap()
    });

    let service = ServiceBuilder::new()
        .concurrency_limit(in_flight_limit)
        .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
        .retry(policy)
        .timeout(Duration::from_secs(timeout))
        .service(http_service);

    let sink = BatchServiceSink::new(service, acker)
        .batched_with_min(
            Buffer::new(gzip),
            batch_size,
            Duration::from_secs(batch_timeout),
        )
        .with(move |e| encode_event(&host_field, e, &encoding));

    Ok(Box::new(sink))
}

pub fn healthcheck(token: String, host: String) -> Result<super::Healthcheck, String> {
    let uri = format!("{}/services/collector/health/1.0", host)
        .parse::<Uri>()
        .map_err(|e| format!("{}", e))?;

    let request = Request::get(uri)
        .header("Authorization", format!("Splunk {}", token))
        .body(Body::empty())
        .unwrap();

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build(https);

    let healthcheck = client
        .request(request)
        .map_err(|err| err.to_string())
        .and_then(|response| match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::BAD_REQUEST => Err("Invalid HEC token".to_string()),
            StatusCode::SERVICE_UNAVAILABLE => Err("HEC is unhealthy, queues are full".to_string()),
            other => Err(format!("Unexpected status: {}", other)),
        });

    Ok(Box::new(healthcheck))
}

pub fn validate_host(host: &String) -> Result<(), String> {
    let uri = Uri::try_from(host).map_err(|e| format!("{}", e))?;

    if let None = uri.scheme_part() {
        Err("Host must include a scheme (https or http)".into())
    } else {
        Ok(())
    }
}

fn encode_event(
    host_field: &Atom,
    event: Event,
    encoding: &Option<Encoding>,
) -> Result<Vec<u8>, ()> {
    let mut event = event.into_log();

    let host = event.get(&host_field).map(|h| h.clone());
    let timestamp = if let Some(ValueKind::Timestamp(ts)) = event.remove(&event::TIMESTAMP) {
        ts.timestamp()
    } else {
        chrono::Utc::now().timestamp()
    };

    let mut body = match (encoding, event.is_structured()) {
        (&Some(Encoding::Json), _) | (_, true) => json!({
            "event": event.all_fields(),
            "fields": event.explicit_fields(),
            "time": timestamp,
        }),
        (&Some(Encoding::Text), _) | (_, false) => json!({
            "event": event[&event::MESSAGE].to_string_lossy(),
            "time": timestamp,
        }),
    };

    if let Some(host) = host {
        let host = host.to_string_lossy();
        body["host"] = json!(host);
    }

    serde_json::to_vec(&body).map_err(|e| panic!("Error encoding json body: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Deserialize, Debug)]
    struct HecEvent {
        time: i64,
        event: HashMap<String, String>,
        fields: HashMap<String, String>,
    }

    #[test]
    fn splunk_encode_event_structured() {
        let host = "host".into();
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_explicit("key".into(), "value".into());

        let bytes = encode_event(&host, event, &None).unwrap();

        let hec_event = serde_json::from_slice::<HecEvent>(&bytes[..]).unwrap();

        let event = &hec_event.event;
        let kv = event.get("key".into()).unwrap();

        assert_eq!(kv, &"value".to_string());
        assert_eq!(
            event[&event::MESSAGE.to_string()],
            "hello world".to_string()
        );
        assert!(event.get(&event::TIMESTAMP.to_string()).is_none());
    }

    #[test]
    fn splunk_validate_host() {
        let valid = "http://localhost:8888".to_string();
        let invalid_scheme = "localhost:8888".to_string();
        let invalid_uri = "iminvalidohnoes".to_string();

        assert_eq!(validate_host(&valid), Ok(()));
        assert!(validate_host(&invalid_scheme).is_err());
        assert!(validate_host(&invalid_uri).is_err());
    }
}

#[cfg(test)]
#[cfg(feature = "splunk-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::buffers::Acker;
    use crate::{
        sinks,
        test_util::{random_lines_with_stream, random_string},
        Event,
    };
    use futures::Sink;
    use serde_json::Value as JsonValue;

    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    #[test]
    fn splunk_insert_message() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk_hec::hec(config(), Acker::Null).unwrap();

        let message = random_string(100);
        let event = Event::from(message.clone());

        let pump = sink.send(event);

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
        assert!(entry.get("message").is_none());
    }

    #[test]
    fn splunk_insert_many() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk_hec::hec(config(), Acker::Null).unwrap();

        let (messages, events) = random_lines_with_stream(100, 10);

        let pump = sink.send_all(events);

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

        let sink = sinks::splunk_hec::hec(config(), Acker::Null).unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event
            .as_mut_log()
            .insert_explicit("asdf".into(), "hello".into());

        let pump = sink.send(event);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap().contains(message.as_str()))
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
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk_hec::hec(config(), Acker::Null).unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event
            .as_mut_log()
            .insert_explicit("asdf".into(), "hello".into());
        event
            .as_mut_log()
            .insert_implicit("host".into(), "example.com:1234".into());

        let pump = sink.send(event);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap().contains(message.as_str()))
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
    fn splunk_configure_hostname() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let config = super::HecSinkConfig {
            host_field: "roast".into(),
            ..config()
        };

        let sink = sinks::splunk_hec::hec(config, Acker::Null).unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event
            .as_mut_log()
            .insert_explicit("asdf".into(), "hello".into());
        event
            .as_mut_log()
            .insert_implicit("host".into(), "example.com:1234".into());
        event
            .as_mut_log()
            .insert_explicit("roast".into(), "beef.example.com:1234".into());

        let pump = sink.send(event);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap().contains(message.as_str()))
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
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        // OK
        {
            let healthcheck =
                sinks::splunk_hec::healthcheck(get_token(), "http://localhost:8088".to_string())
                    .unwrap();
            rt.block_on(healthcheck).unwrap();
        }

        // Server not listening at address
        {
            let healthcheck =
                sinks::splunk_hec::healthcheck(get_token(), "http://localhost:1111".to_string())
                    .unwrap();

            rt.block_on(healthcheck).unwrap_err();
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
            let healthcheck =
                sinks::splunk_hec::healthcheck(get_token(), "http://503.returnco.de".to_string())
                    .unwrap();
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
            .form(&[("search", "search *"), ("exec_mode", "oneshot"), ("f", "*")])
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
            host_field: "host".into(),
            compression: Some(Compression::None),
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

        let token = entries[0]["content"]["token"].as_str().unwrap().to_owned();

        token
    }
}
