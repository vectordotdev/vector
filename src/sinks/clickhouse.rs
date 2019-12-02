use crate::{
    buffers::Acker,
    event::Event,
    sinks::util::{
        http::{HttpRetryLogic, HttpService, Response},
        retries::RetryLogic,
        tls::{TlsOptions, TlsSettings},
        BatchConfig, Buffer, Compression, SinkExt, TowerRequestConfig,
    },
    topology::config::{DataType, SinkConfig, SinkDescription},
};
use futures::{stream::iter_ok, Future, Sink};
use headers::HeaderMapExt;
use http::StatusCode;
use http::{Method, Uri};
use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::borrow::Cow;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ClickhouseConfig {
    pub host: String,
    pub table: String,
    pub database: Option<String>,
    pub compression: Option<Compression>,
    pub basic_auth: Option<ClickHouseBasicAuthConfig>,
    #[serde(default, flatten)]
    pub batch: BatchConfig,
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
    fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = clickhouse(self.clone(), acker)?;
        let healtcheck = healthcheck(self.host.clone(), self.basic_auth.clone());

        Ok((sink, healtcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "clickhouse"
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ClickHouseBasicAuthConfig {
    pub password: String,
    pub user: String,
}

impl ClickHouseBasicAuthConfig {
    fn apply(&self, header_map: &mut http::header::HeaderMap) {
        let auth = headers::Authorization::basic(&self.user, &self.password);
        header_map.typed_insert(auth)
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

    let batch = config.batch.unwrap_or(bytesize::mib(10u64), 1);
    let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

    let basic_auth = config.basic_auth.clone();

    let uri = encode_uri(&host, &database, &table)?;
    let tls_settings = TlsSettings::from_options(&config.tls)?;

    let http_service =
        HttpService::builder()
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

                if let Some(auth) = &basic_auth {
                    auth.apply(request.headers_mut());
                }

                request
            });

    let sink = request
        .batch_sink(
            ClickhouseRetryLogic {
                inner: HttpRetryLogic,
            },
            http_service,
            acker,
        )
        .batched_with_min(Buffer::new(gzip), &batch)
        .with_flat_map(move |event: Event| iter_ok(encode_event(event)));

    Ok(Box::new(sink))
}

fn encode_event(event: Event) -> Option<Vec<u8>> {
    let mut body =
        serde_json::to_vec(&event.as_log().all_fields()).expect("Events should be valid json!");
    body.push(b'\n');
    Some(body)
}

fn healthcheck(host: String, basic_auth: Option<ClickHouseBasicAuthConfig>) -> super::Healthcheck {
    // TODO: check if table exists?
    let uri = format!("{}/?query=SELECT%201", host);
    let mut request = Request::get(uri).body(Body::empty()).unwrap();

    if let Some(auth) = &basic_auth {
        auth.apply(request.headers_mut());
    }

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

    fn should_retry_response(&self, response: &Self::Response) -> Option<Cow<str>> {
        match response.status() {
            StatusCode::INTERNAL_SERVER_ERROR => {
                let body = response.body();

                // Currently, clickhouse returns 500's incorrect data and type mismatch errors.
                // This attempts to check if the body starts with `Code: {code_num}` and to not
                // retry those errors.
                //
                // Reference: https://github.com/timberio/vector/pull/693#issuecomment-517332654
                if body.starts_with(b"Code: 117") || body.starts_with(b"Code: 53") {
                    None
                } else {
                    Some(String::from_utf8_lossy(body).to_string().into())
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
            batch: BatchConfig {
                batch_size: Some(1),
                batch_timeout: None,
            },
            request: TowerRequestConfig {
                request_retry_attempts: Some(1),
                ..Default::default()
            },
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
            batch: BatchConfig {
                batch_size: Some(1),
                batch_timeout: None,
            },
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
