use crate::{
    dns::Resolver,
    event::{self, Event, ValueKind},
    sinks::util::{
        http::{https_client, HttpRetryLogic, HttpService},
        tls::{TlsOptions, TlsSettings},
        BatchConfig, Buffer, Compression, SinkExt, TowerRequestConfig,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use bytes::Bytes;
use futures::{stream::iter_ok, Future, Sink};
use http::{HttpTryFrom, Method, Request, StatusCode, Uri};
use hyper::Body;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::{ResultExt, Snafu};
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
    #[serde(default = "default_host_field")]
    pub host_field: Atom,
    pub encoding: Encoding,
    pub compression: Option<Compression>,
    #[serde(default, flatten)]
    pub batch: BatchConfig,
    #[serde(flatten)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        request_in_flight_limit: Some(10),
        request_rate_limit_num: Some(10),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Json,
}

fn default_host_field() -> Atom {
    event::HOST.clone()
}

inventory::submit! {
    SinkDescription::new::<HecSinkConfig>("splunk_hec")
}

#[typetag::serde(name = "splunk_hec")]
impl SinkConfig for HecSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        validate_host(&self.host)?;
        let healthcheck = healthcheck(&self, cx.resolver())?;
        let sink = hec(self.clone(), cx)?;

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "splunk_hec"
    }
}

pub fn hec(config: HecSinkConfig, cx: SinkContext) -> crate::Result<super::RouterSink> {
    let host = config.host.clone();
    let token = config.token.clone();
    let host_field = config.host_field;

    let gzip = match config.compression.unwrap_or(Compression::None) {
        Compression::None => false,
        Compression::Gzip => true,
    };
    let batch = config.batch.unwrap_or(bytesize::mib(1u64), 1);
    let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
    let encoding = config.encoding.clone();

    let uri = format!("{}/services/collector/event", host)
        .parse::<Uri>()
        .context(super::UriParseError)?;
    let token = Bytes::from(format!("Splunk {}", token));

    let tls_settings = TlsSettings::from_options(&config.tls)?;

    let http_service = HttpService::builder(cx.resolver())
        .tls_settings(tls_settings)
        .build(move |body: Vec<u8>| {
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

    let sink = request
        .batch_sink(HttpRetryLogic, http_service, cx.acker())
        .batched_with_min(Buffer::new(gzip), &batch)
        .with_flat_map(move |e| iter_ok(encode_event(&host_field, e, &encoding)));

    Ok(Box::new(sink))
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Invalid HEC token"))]
    InvalidToken,
    #[snafu(display("Queues are full"))]
    QueuesFull,
}

pub fn healthcheck(
    config: &HecSinkConfig,
    resolver: Resolver,
) -> crate::Result<super::Healthcheck> {
    let uri = format!("{}/services/collector/health/1.0", config.host)
        .parse::<Uri>()
        .context(super::UriParseError)?;

    let request = Request::get(uri)
        .header("Authorization", format!("Splunk {}", config.token))
        .body(Body::empty())
        .unwrap();

    let tls = TlsSettings::from_options(&config.tls)?;
    let client = https_client(resolver, tls)?;

    let healthcheck = client
        .request(request)
        .map_err(|err| err.into())
        .and_then(|response| match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::BAD_REQUEST => Err(HealthcheckError::InvalidToken.into()),
            StatusCode::SERVICE_UNAVAILABLE => Err(HealthcheckError::QueuesFull.into()),
            other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
        });

    Ok(Box::new(healthcheck))
}

pub fn validate_host(host: &str) -> crate::Result<()> {
    let uri = Uri::try_from(host).context(super::UriParseError)?;

    match uri.scheme_part() {
        Some(_) => Ok(()),
        None => Err(Box::new(BuildError::UriMissingScheme)),
    }
}

fn encode_event(host_field: &Atom, event: Event, encoding: &Encoding) -> Option<Vec<u8>> {
    let mut event = event.into_log();

    let host = event.get(&host_field).cloned();
    let timestamp = if let Some(ValueKind::Timestamp(ts)) = event.remove(&event::TIMESTAMP) {
        ts.timestamp()
    } else {
        chrono::Utc::now().timestamp()
    };

    let mut body = match encoding {
        Encoding::Json => json!({
            "fields": event.explicit_fields(),
            "event": event.unflatten(),
            "time": timestamp,
        }),
        Encoding::Text => json!({
            "event": event.get(&event::MESSAGE).map(|v| v.to_string_lossy()).unwrap_or_else(|| "".into()),
            "time": timestamp,
        }),
    };

    if let Some(host) = host {
        let host = host.to_string_lossy();
        body["host"] = json!(host);
    }

    serde_json::to_vec(&body)
        .map_err(|e| error!("Error encoding json body: {}", e))
        .ok()
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
    fn splunk_encode_event_json() {
        let host = "host".into();
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_explicit("key".into(), "value".into());

        let bytes = encode_event(&host, event, &Encoding::Json).unwrap();

        let hec_event = serde_json::from_slice::<HecEvent>(&bytes[..]).unwrap();

        let event = &hec_event.event;
        let kv = event.get(&"key".to_string()).unwrap();

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

        assert!(validate_host(&valid).is_ok());
        assert!(validate_host(&invalid_scheme).is_err());
        assert!(validate_host(&invalid_uri).is_err());
    }
}

#[cfg(test)]
#[cfg(feature = "splunk-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        assert_downcast_matches, sinks,
        test_util::{random_lines_with_stream, random_string, runtime},
        topology::config::SinkContext,
        Event,
    };
    use futures::Sink;
    use http::StatusCode;
    use serde_json::Value as JsonValue;
    use std::net::SocketAddr;
    use warp::Filter;

    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    #[test]
    fn splunk_insert_message() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let sink = sinks::splunk_hec::hec(config(Encoding::Text), cx).unwrap();

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
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let sink = sinks::splunk_hec::hec(config(Encoding::Text), cx).unwrap();

        let (messages, events) = random_lines_with_stream(100, 10);

        let pump = sink.send_all(events);

        let _ = rt.block_on(pump).unwrap();

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
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let sink = sinks::splunk_hec::hec(config(Encoding::Json), cx).unwrap();

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

        let sink = sinks::splunk_hec::hec(config(Encoding::Json), cx).unwrap();

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
    fn splunk_configure_hostname() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let config = super::HecSinkConfig {
            host_field: "roast".into(),
            ..config(Encoding::Json)
        };

        let sink = sinks::splunk_hec::hec(config, cx).unwrap();

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
        let resolver = crate::dns::Resolver::new(Vec::new(), rt.executor()).unwrap();

        // OK
        {
            let config = config(Encoding::Text);
            let healthcheck = sinks::splunk_hec::healthcheck(&config, resolver.clone()).unwrap();
            rt.block_on(healthcheck).unwrap();
        }

        // Server not listening at address
        {
            let config = HecSinkConfig {
                host: "http://localhost:1111".to_string(),
                ..config(Encoding::Text)
            };
            let healthcheck = sinks::splunk_hec::healthcheck(&config, resolver.clone()).unwrap();

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
            let config = HecSinkConfig {
                host: "http://localhost:5503".to_string(),
                ..config(Encoding::Text)
            };

            let unhealthy = warp::any()
                .map(|| warp::reply::with_status("i'm sad", StatusCode::SERVICE_UNAVAILABLE));
            let server = warp::serve(unhealthy).bind("0.0.0.0:5503".parse::<SocketAddr>().unwrap());
            rt.spawn(server);

            let healthcheck = sinks::splunk_hec::healthcheck(&config, resolver).unwrap();
            assert_downcast_matches!(
                rt.block_on(healthcheck).unwrap_err(),
                HealthcheckError,
                HealthcheckError::QueuesFull
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

    fn config(encoding: Encoding) -> super::HecSinkConfig {
        super::HecSinkConfig {
            host: "http://localhost:8088/".into(),
            token: get_token(),
            host_field: "host".into(),
            compression: Some(Compression::None),
            encoding,
            batch: BatchConfig {
                batch_size: Some(1),
                batch_timeout: None,
            },
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
