use crate::{
    buffers::Acker,
    event::Event,
    sinks::util::{
        http::{HttpRetryLogic, HttpService},
        retries::FixedRetryPolicy,
        BatchServiceSink, Buffer, Compression, SinkExt,
    },
    topology::config::{DataType, SinkConfig},
};
use futures::{Future, Sink};
use http::{Method, Uri};
use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
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
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = clickhouse(self.clone(), acker)?;
        let healtcheck = healthcheck(self.host.clone());

        Ok((sink, healtcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }
}

fn clickhouse(config: ClickhouseConfig, acker: Acker) -> Result<super::RouterSink, String> {
    let mut host = config.host.clone();
    let table = config.table.clone();
    let database = config.database.clone().unwrap_or("default".into());

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
        HttpRetryLogic,
    );

    if !host.ends_with("/") {
        host.push('/');
    }

    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "query",
            format!(
                "INSERT INTO {}.{} FORMAT JSONEachRow",
                database, table.replace("\"", "\\\"")
            )
            .as_str(),
          )
        .finish();

    let url = format!("{}?{}", host, query);
    let uri = url
        .parse::<Uri>()
        .map_err(|e| format!("Unable to parse host as URI: {}", e))?;

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
            let mut body = serde_json::to_vec(&event.as_log().all_fields()).unwrap();
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
        .map_err(|err| err.to_string())
        .and_then(|response| {
            if response.status() == hyper::StatusCode::OK {
                Ok(())
            } else {
                Err(format!("Unexpected status: {}", response.status()))
            }
        });

    Box::new(healthcheck)
}

#[cfg(test)]
#[cfg(feature = "clickhouse-integration-tests")]
mod tests {
    use super::*;
    use crate::buffers::Acker;
    use crate::{
        test_util::{block_on, random_string},
        topology::config::SinkConfig,
        Event,
    };
    use futures::Sink;
    use serde_json::Value;

    #[test]
    fn insert_events() {
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
        client.create_table(&table);

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

        fn create_table(&self, table: &str) {
            let mut response = self
                .client
                .post(&self.host)
                .body(format!(
                    "CREATE TABLE {}
                     (host String, timestamp String, message String)
                     ENGINE = MergeTree()
                     PARTITION BY substring(timestamp, 1, 7)
                     ORDER BY (host, timestamp);",
                    table,
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
