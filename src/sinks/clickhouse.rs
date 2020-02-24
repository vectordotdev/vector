use crate::{
    dns::Resolver,
    event::{Event, Value},
    sinks::util::{
        http::{https_client, Auth, HttpRetryLogic, HttpService, Response},
        retries::{RetryAction, RetryLogic},
        BatchBytesConfig, Buffer, Compression, SinkExt, TowerRequestConfig,
    },
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::{stream::iter_ok, Future, Sink};
use http::StatusCode;
use http::{Method, Uri};
use hyper::{Body, Request};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum TimestampFormat {
    Unix,
    RFC3339,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfig {
    pub timestamp_format: Option<TimestampFormat>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ClickhouseConfig {
    pub host: String,
    pub table: String,
    pub database: Option<String>,
    pub compression: Option<Compression>,
    pub encoding: EncodingConfig,
    #[serde(default)]
    pub batch: BatchBytesConfig,
    pub auth: Option<Auth>,
    #[serde(flatten)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        ..Default::default()
    };
}

inventory::submit! {
    SinkDescription::new::<ClickhouseConfig>("clickhouse")
}

#[typetag::serde(name = "clickhouse")]
impl SinkConfig for ClickhouseConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let healtcheck = healthcheck(cx.resolver(), &self)?;
        let sink = clickhouse(self.clone(), cx)?;

        Ok((sink, healtcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "clickhouse"
    }
}

fn clickhouse(config: ClickhouseConfig, cx: SinkContext) -> crate::Result<super::RouterSink> {
    let host = config.host.clone();
    let database = config.database.clone().unwrap_or("default".into());
    let table = config.table.clone();

    let gzip = match config.compression.unwrap_or(Compression::Gzip) {
        Compression::None => false,
        Compression::Gzip => true,
    };

    let batch = config.batch.unwrap_or(bytesize::mib(10u64), 1);
    let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

    let auth = config.auth.clone();

    let uri = encode_uri(&host, &database, &table)?;
    let tls_settings = TlsSettings::from_options(&config.tls)?;

    let http_service = HttpService::builder(cx.resolver())
        .tls_settings(tls_settings)
        .build(move |body: Vec<u8>| {
            let mut builder = hyper::Request::builder();
            builder.method(Method::POST);
            builder.uri(uri.clone());

            builder.header("Content-Type", "application/x-ndjson");

            if gzip {
                builder.header("Content-Encoding", "gzip");
            }

            let mut request = builder.body(body).unwrap();

            if let Some(auth) = &auth {
                auth.apply(&mut request);
            }

            request
        });

    let sink = request
        .batch_sink(
            ClickhouseRetryLogic {
                inner: HttpRetryLogic,
            },
            http_service,
            cx.acker(),
        )
        .batched_with_min(Buffer::new(gzip), &batch)
        .with_flat_map(move |event: Event| iter_ok(encode_event(&config, event)));

    Ok(Box::new(sink))
}

fn encode_event(config: &ClickhouseConfig, mut event: Event) -> Option<Vec<u8>> {
    match config.encoding.timestamp_format {
        Some(TimestampFormat::Unix) => {
            let mut unix_timestamps = Vec::new();
            for (k, v) in event.as_log().all_fields() {
                if let Value::Timestamp(ts) = v {
                    unix_timestamps.push((k.clone(), Value::Integer(ts.timestamp())));
                }
            }
            for (k, v) in unix_timestamps.pop() {
                event.as_mut_log().insert(k, v);
            }
        }
        // RFC3339 is the default serialization of a timestamp.
        Some(TimestampFormat::RFC3339) | None => {}
    }
    let mut body =
        serde_json::to_vec(&event.as_log().all_fields()).expect("Events should be valid json!");
    body.push(b'\n');
    Some(body)
}

fn healthcheck(resolver: Resolver, config: &ClickhouseConfig) -> crate::Result<super::Healthcheck> {
    // TODO: check if table exists?
    let uri = format!("{}/?query=SELECT%201", config.host);
    let mut request = Request::get(uri).body(Body::empty()).unwrap();

    if let Some(auth) = &config.auth {
        auth.apply(&mut request);
    }

    let tls = TlsSettings::from_options(&config.tls)?;
    let client = https_client(resolver, tls)?;
    let healthcheck = client
        .request(request)
        .map_err(|err| err.into())
        .and_then(|response| match response.status() {
            hyper::StatusCode::OK => Ok(()),
            status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
        });

    Ok(Box::new(healthcheck))
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

    let url = if host.ends_with('/') {
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

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        match response.status() {
            StatusCode::INTERNAL_SERVER_ERROR => {
                let body = response.body();

                // Currently, clickhouse returns 500's incorrect data and type mismatch errors.
                // This attempts to check if the body starts with `Code: {code_num}` and to not
                // retry those errors.
                //
                // Reference: https://github.com/timberio/vector/pull/693#issuecomment-517332654
                // Error code definitions: https://github.com/ClickHouse/ClickHouse/blob/master/dbms/src/Common/ErrorCodes.cpp
                if body.starts_with(b"Code: 117") {
                    RetryAction::DontRetry("incorrect data".into())
                } else if body.starts_with(b"Code: 53") {
                    RetryAction::DontRetry("type mismatch".into())
                } else {
                    RetryAction::Retry(String::from_utf8_lossy(body).to_string().into())
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
        event,
        event::Event,
        test_util::{random_string, runtime},
        topology::config::{SinkConfig, SinkContext},
    };
    use futures::Sink;
    use serde_json::Value;
    use std::time::Duration;
    use tokio::util::FutureExt;

    #[test]
    fn insert_events() {
        crate::test_util::trace_init();
        let mut rt = runtime();

        let table = gen_table();
        let host = String::from("http://localhost:8123");

        let config = ClickhouseConfig {
            host: host.clone(),
            table: table.clone(),
            compression: Some(Compression::None),
            batch: BatchBytesConfig {
                max_size: Some(1),
                timeout_secs: None,
            },
            request: TowerRequestConfig {
                retry_attempts: Some(1),
                ..Default::default()
            },
            ..Default::default()
        };

        let client = ClickhouseClient::new(host);
        client.create_table(&table, "host String, timestamp String, message String");

        let (sink, _hc) = config.build(SinkContext::new_test(rt.executor())).unwrap();

        let mut input_event = Event::from("raw log line");
        input_event.as_mut_log().insert("host", "example.com");

        let pump = sink.send(input_event.clone());
        rt.block_on(pump).unwrap();

        let output = client.select_all(&table);
        assert_eq!(1, output.rows);

        let expected = serde_json::to_value(input_event.into_log().all_fields()).unwrap();
        assert_eq!(expected, output.data[0]);
    }

    #[test]
    fn insert_events_unix_timestamps() {
        crate::test_util::trace_init();
        let mut rt = runtime();

        let table = gen_table();
        let host = String::from("http://localhost:8123");

        let config = ClickhouseConfig {
            host: host.clone(),
            table: table.clone(),
            compression: Some(Compression::None),
            encoding: EncodingConfig {
                timestamp_format: Some(TimestampFormat::Unix),
            },
            batch: BatchBytesConfig {
                max_size: Some(1),
                timeout_secs: None,
            },
            request: TowerRequestConfig {
                retry_attempts: Some(1),
                ..Default::default()
            },
            ..Default::default()
        };

        let client = ClickhouseClient::new(host);
        client.create_table(
            &table,
            "host String, timestamp DateTime('Europe/London'), message String",
        );

        let (sink, _hc) = config.build(SinkContext::new_test(rt.executor())).unwrap();

        let mut input_event = Event::from("raw log line");
        input_event.as_mut_log().insert("host", "example.com");

        let pump = sink.send(input_event.clone());
        rt.block_on(pump).unwrap();

        let output = client.select_all(&table);
        assert_eq!(1, output.rows);

        let exp_event = input_event.as_mut_log();
        exp_event.insert(
            event::log_schema().timestamp_key().clone(),
            format!(
                "{}",
                exp_event
                    .get(&event::log_schema().timestamp_key())
                    .unwrap()
                    .as_timestamp()
                    .unwrap()
                    .format("%Y-%m-%d %H:%M:%S")
            ),
        );

        let expected = serde_json::to_value(exp_event.all_fields()).unwrap();
        assert_eq!(expected, output.data[0]);
    }

    #[test]
    fn no_retry_on_incorrect_data() {
        crate::test_util::trace_init();
        let mut rt = runtime();

        let table = gen_table();
        let host = String::from("http://localhost:8123");

        let config = ClickhouseConfig {
            host: host.clone(),
            table: table.clone(),
            compression: Some(Compression::None),
            batch: BatchBytesConfig {
                max_size: Some(1),
                timeout_secs: None,
            },
            ..Default::default()
        };

        let client = ClickhouseClient::new(host);
        // the event contains a message field, but its being omited to
        // fail the request.
        client.create_table(&table, "host String, timestamp String");

        let (sink, _hc) = config.build(SinkContext::new_test(rt.executor())).unwrap();

        let mut input_event = Event::from("raw log line");
        input_event.as_mut_log().insert("host", "example.com");

        let pump = sink.send(input_event.clone());

        // Retries should go on forever, so if we are retrying incorrectly
        // this timeout should trigger.
        rt.block_on(pump.timeout(Duration::from_secs(5))).unwrap();
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
