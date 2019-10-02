use crate::{
    buffers::Acker,
    event::Event,
    sinks::util::{
        http::{HttpRetryLogic, HttpService, Response},
        retries::{FixedRetryPolicy, RetryLogic},
        BatchServiceSink, Buffer, Compression, SinkExt,
    },
    topology::config::{DataType, SinkConfig},
};
use futures::{Future, Sink};
use http::StatusCode;
use http::{Method, Uri};
use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::borrow::Cow;
use std::time::Duration;
use tower::ServiceBuilder;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ClickhouseConfig {
    pub host: String,
    pub table: String,
    pub database: Option<String>,
    pub batch_size: Option<usize>,
    pub batch_timeout: Option<u64>,
    pub compression: Option<Compression>,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,
}

#[typetag::serde(name = "clickhouse")]
impl SinkConfig for ClickhouseConfig {
    fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = clickhouse(self.clone(), acker)?;
        let healtcheck = healthcheck(self.host.clone());

        Ok((sink, healtcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }
}

fn clickhouse(config: ClickhouseConfig, acker: Acker) -> crate::Result<super::RouterSink> {
    let host = config.host.clone();
    let database = config.database.clone().unwrap_or("default".into());
    let table = config.table.clone();

    let gzip = match config.compression.unwrap_or(Compression::Gzip) {
        Compression::None => false,
        Compression::Gzip => true,
    };

    let batch_size = config.batch_size.unwrap_or(bytesize::mib(10u64) as usize);
    let batch_timeout = config.batch_timeout.unwrap_or(1);

    let timeout = config.request_timeout_secs.unwrap_or(60);
    let in_flight_limit = config.request_in_flight_limit.unwrap_or(5);
    let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
    let rate_limit_num = config.request_rate_limit_num.unwrap_or(5);
    let retry_attempts = config.request_retry_attempts.unwrap_or(usize::max_value());
    let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);

    let policy = FixedRetryPolicy::new(
        retry_attempts,
        Duration::from_secs(retry_backoff_secs),
        ClickhouseRetryLogic {
            inner: HttpRetryLogic,
        },
    );

    let uri = encode_uri(&host, &database, &table)?;

    let http_service = HttpService::new(move |body: Vec<u8>| {
        let mut builder = hyper::Request::builder();
        builder.method(Method::POST);
        builder.uri(uri.clone());

        builder.header("Content-Type", "application/x-ndjson");

        if gzip {
            builder.header("Content-Encoding", "gzip");
        }

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
        .with(move |event: Event| {
            let mut body = serde_json::to_vec(&event.as_log().all_fields())
                .expect("Events should be valid json!");
            body.push(b'\n');
            Ok(body)
        });

    Ok(Box::new(sink))
}

fn healthcheck(host: String) -> super::Healthcheck {
    // TODO: check if table exists?
    let uri = format!("{}/?query=SELECT%201", host);
    let request = Request::get(uri).body(Body::empty()).unwrap();

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build(https);
    let healthcheck = client
        .request(request)
        .map_err(|err| err.into())
        .and_then(|response| match response.status() {
            hyper::StatusCode::OK => Ok(()),
            status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
        });

    Box::new(healthcheck)
}

fn encode_uri(host: &str, database: &str, table: &str) -> crate::Result<Uri> {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "query",
            format!(
                "INSERT INTO \"{}\".\"{}\" FORMAT JSONEachRow",
                database,
                table.replace("\"", "\\\"")
            )
            .as_str(),
        )
        .finish();

    let url = if host.ends_with("/") {
        format!("{}?{}", host, query)
    } else {
        format!("{}/?{}", host, query)
    };

    Ok(url.parse::<Uri>().context(super::UriParseError)?)
}

#[derive(Clone)]
struct ClickhouseRetryLogic {
    inner: HttpRetryLogic,
}

impl RetryLogic for ClickhouseRetryLogic {
    type Response = Response;
    type Error = hyper::Error;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        self.inner.is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> Option<Cow<str>> {
        match response.status() {
            StatusCode::INTERNAL_SERVER_ERROR => {
                let body = response.body();

                // Currently, clickhouse returns 500's incorrect data and type mismatch errors.
                // This attempts to check if the body starts with `Code: {code_num}` and to not
                // retry those errors.
                //
                // Reference: https://github.com/timberio/vector/pull/693#issuecomment-517332654
                match body.starts_with(b"Code: 117") || body.starts_with(b"Code: 53") {
                    false => Some(String::from_utf8_lossy(body).to_string().into()),
                    true => None,
                }
            }
            _ => self.inner.should_retry_response(response),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_valid() {
        let uri = encode_uri("http://localhost:80", "my_database", "my_table").unwrap();
        assert_eq!(uri, "http://localhost:80/?query=INSERT+INTO+%22my_database%22.%22my_table%22+FORMAT+JSONEachRow");

        let uri = encode_uri("http://localhost:80", "my_database", "my_\"table\"").unwrap();
        assert_eq!(uri, "http://localhost:80/?query=INSERT+INTO+%22my_database%22.%22my_%5C%22table%5C%22%22+FORMAT+JSONEachRow");
    }

    #[test]
    fn encode_invalid() {
        encode_uri("localhost:80", "my_database", "my_table").unwrap_err();
    }
}

#[cfg(test)]
#[cfg(feature = "clickhouse-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        buffers::Acker,
        test_util::{block_on, random_string},
        topology::config::SinkConfig,
        Event,
    };
    use futures::Sink;
    use serde_json::Value;
    use std::time::Duration;
    use tokio::util::FutureExt;

    #[test]
    fn insert_events() {
        crate::test_util::trace_init();

        let table = gen_table();
        let host = String::from("http://localhost:8123");

        let config = ClickhouseConfig {
            host: host.clone(),
            table: table.clone(),
            compression: Some(Compression::None),
            batch_size: Some(1),
            request_retry_attempts: Some(1),
            ..Default::default()
        };

        let client = ClickhouseClient::new(host);
        client.create_table(&table, "host String, timestamp String, message String");

        let (sink, _hc) = config.build(Acker::Null).unwrap();

        let mut input_event = Event::from("raw log line");
        input_event
            .as_mut_log()
            .insert_explicit("host".into(), "example.com".into());

        let pump = sink.send(input_event.clone());
        block_on(pump).unwrap();

        let output = client.select_all(&table);
        assert_eq!(1, output.rows);

        let expected = serde_json::to_value(input_event.into_log().all_fields()).unwrap();
        assert_eq!(expected, output.data[0]);
    }

    #[test]
    fn no_retry_on_incorrect_data() {
        crate::test_util::trace_init();

        let table = gen_table();
        let host = String::from("http://localhost:8123");

        let config = ClickhouseConfig {
            host: host.clone(),
            table: table.clone(),
            compression: Some(Compression::None),
            batch_size: Some(1),
            ..Default::default()
        };

        let client = ClickhouseClient::new(host);
        // the event contains a message field, but its being omited to
        // fail the request.
        client.create_table(&table, "host String, timestamp String");

        let (sink, _hc) = config.build(Acker::Null).unwrap();

        let mut input_event = Event::from("raw log line");
        input_event
            .as_mut_log()
            .insert_explicit("host".into(), "example.com".into());

        let pump = sink.send(input_event.clone());

        // Retries should go on forever, so if we are retrying incorrectly
        // this timeout should trigger.
        block_on(pump.timeout(Duration::from_secs(5))).unwrap();
    }

    struct ClickhouseClient {
        host: String,
        client: reqwest::Client,
    }

    impl ClickhouseClient {
        fn new(host: String) -> Self {
            ClickhouseClient {
                host,
                client: reqwest::Client::new(),
            }
        }

        fn create_table(&self, table: &str, schema: &str) {
            let mut response = self
                .client
                .post(&self.host)
                //
                .body(format!(
                    "CREATE TABLE {}
                     ({})
                     ENGINE = MergeTree()
                     PARTITION BY substring(timestamp, 1, 7)
                     ORDER BY (host, timestamp);",
                    table, schema
                ))
                .send()
                .unwrap();
            if !response.status().is_success() {
                panic!("create table failed: {}", response.text().unwrap())
            }
        }

        fn select_all(&self, table: &str) -> QueryResponse {
            let mut response = self
                .client
                .post(&self.host)
                .body(format!("SELECT * FROM {} FORMAT JSON", table))
                .send()
                .unwrap();
            if !response.status().is_success() {
                panic!("select all failed: {}", response.text().unwrap())
            } else {
                if let Ok(value) = response.json() {
                    value
                } else {
                    panic!("json failed: {:?}", response.text().unwrap());
                }
            }
        }
    }

    #[derive(Debug, Deserialize)]
    struct QueryResponse {
        data: Vec<Value>,
        meta: Vec<Value>,
        rows: usize,
        statistics: Stats,
    }

    #[derive(Debug, Deserialize)]
    struct Stats {
        bytes_read: usize,
        elapsed: f64,
        rows_read: usize,
    }

    fn gen_table() -> String {
        format!("test_{}", random_string(10).to_lowercase())
    }
}
