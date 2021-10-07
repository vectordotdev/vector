use super::{healthcheck_response, GcpAuthConfig, GcpCredentials, Scope};
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    http::{HttpClient, HttpError},
    sinks::{
        util::{
            batch::{BatchConfig, BatchSettings},
            encoding::{EncodingConfigWithDefault, EncodingConfiguration},
            http::{BatchedHttpSink, HttpRetryLogic, HttpSink},
            retries::{RetryAction, RetryLogic},
            sink, BoxedRawValue, JsonArrayBuffer, TowerRequestConfig,
        },
        Healthcheck, UriParseError, VectorSink,
    },
    tls::{TlsOptions, TlsSettings},
};
use bytes::Bytes;
use futures::{FutureExt, SinkExt};
use http::{StatusCode, Uri};
use hyper::{Body, Request};
use indoc::indoc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use snafu::{ResultExt, Snafu};
use uuid::Uuid;

const NAME: &str = "gcp_bigquery";
const ENDPOINT: &str = "https://bigquery.googleapis.com";
const BASE_URL: &str = "/bigquery/v2/";

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

#[derive(Debug, Snafu)]
enum BigqueryError {
    #[snafu(display("Table not found {}.{}.{} not found", project, dataset, table))]
    TableNotFound {
        project: String,
        dataset: String,
        table: String,
    },
}

fn default_endpoint() -> String {
    ENDPOINT.to_string()
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct BigquerySinkConfig {
    #[serde(default = "default_endpoint")]
    endpoint: String,
    project: String,
    dataset: String,
    table: String,
    #[serde(default)]
    include_insert_id: bool,
    #[serde(default)]
    ignore_unknown_values: bool,
    #[serde(default)]
    skip_invalid_rows: bool,
    template_suffix: Option<String>,
    #[serde(default)]
    request: TowerRequestConfig,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(flatten)]
    auth: Option<GcpAuthConfig>,
    #[serde(default)]
    batch: BatchConfig,
    tls: Option<TlsOptions>,
}

inventory::submit! {
    SinkDescription::new::<BigquerySinkConfig>(NAME)
}

impl GenerateConfig for BigquerySinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            project = "my-project"
            dataset = "my-dataset"
            table = "my-table"
            ignore_unknown_values = true
            credentials_path = "/path/to/credentials.json"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_bigquery")]
impl SinkConfig for BigquerySinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig {
            // BigQuery returns intermittent 502 errors, which require waiting 30 seconds
            retry_initial_backoff_secs: Some(30),
            ..Default::default()
        });
        let tls_settings = TlsSettings::from_options(&self.tls)?;

        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let sink = BigquerySink::from_config(self).await?;
        let batch_settings = BatchSettings::default()
            // BigQuery has a max request size of 10MB
            .bytes(bytesize::mib(8u64))
            .events(1000)
            .timeout(1)
            .parse_config(self.batch)?;

        let uri = sink.uri("").expect("failed to parse uri");
        let healthcheck = healthcheck(client.clone(), uri, sink.creds.clone()).boxed();

        let sink = BatchedHttpSink::with_logic(
            sink,
            JsonArrayBuffer::new(batch_settings.size),
            BigqueryRetryLogic::default(),
            request_settings,
            batch_settings.timeout,
            client,
            cx.acker(),
            sink::StdServiceLogic::default(),
        )
        .sink_map_err(|error| error!(message = "Fatal gcp_bigquery sink error.", %error));

        Ok((VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        NAME
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Invalid credentials"))]
    InvalidCredentials,
    #[snafu(display("Table not found {} not found", uri))]
    HealthcheckTableNotFound { uri: String },
}

struct BigquerySink {
    include_insert_id: bool,
    ignore_unknown_values: bool,
    skip_invalid_rows: bool,
    template_suffix: Option<String>,
    api_key: Option<String>,
    creds: Option<GcpCredentials>,
    uri_base: String,
    encoding: EncodingConfigWithDefault<Encoding>,
}

impl BigquerySink {
    async fn from_config(config: &BigquerySinkConfig) -> crate::Result<Self> {
        let (creds, api_key) = match &config.auth {
            Some(auth) => (
                auth.make_credentials(Scope::BigQuery).await?,
                auth.api_key.clone(),
            ),
            None => (None, None),
        };

        let uri_base = format!(
            "{}{}projects/{}/datasets/{}/tables/{}",
            config.endpoint, BASE_URL, config.project, config.dataset, config.table
        );

        Ok(BigquerySink {
            include_insert_id: config.include_insert_id,
            ignore_unknown_values: config.ignore_unknown_values,
            skip_invalid_rows: config.skip_invalid_rows,
            template_suffix: config.template_suffix.clone(),
            api_key,
            creds,
            uri_base,
            encoding: config.encoding.clone(),
        })
    }

    fn uri(&self, suffix: &str) -> crate::Result<Uri> {
        let mut uri = format!("{}{}", self.uri_base, suffix);
        if let Some(key) = &self.api_key {
            uri = format!("{}?key={}", uri, key);
        }
        uri.parse::<Uri>()
            .context(UriParseError)
            .map_err(Into::into)
    }
}

#[async_trait::async_trait]
impl HttpSink for BigquerySink {
    type Input = Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);
        let log = event.into_log();
        let json = serde_json::json!(&log);
        Some(json)
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        let insert_id = if self.include_insert_id {
            Some(Uuid::new_v4().to_hyphenated().to_string())
        } else {
            None
        };
        let insert_request_rows = events
            .into_iter()
            .map(|v| InsertAllRequestRows {
                insert_id: insert_id.clone(),
                json: v,
            })
            .collect();
        let insert_request = InsertAllRequest {
            ignore_unknown_values: self.ignore_unknown_values,
            kind: None,
            rows: insert_request_rows,
            skip_invalid_rows: self.skip_invalid_rows,
            template_suffix: self.template_suffix.clone(),
        };
        let body = serde_json::to_vec(&insert_request).unwrap();
        let uri = self.uri("/insertAll").expect("failed to parse uri");
        let builder = Request::post(uri).header("Content-Type", "application/json");

        let mut request = builder.body(body).unwrap();
        if let Some(creds) = &self.creds {
            creds.apply(&mut request);
        }

        Ok(request)
    }
}

#[derive(Debug, Default, Clone)]
struct BigqueryRetryLogic {
    inner: HttpRetryLogic,
}

impl RetryLogic for BigqueryRetryLogic {
    type Error = HttpError;
    type Response = http::Response<Bytes>;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        self.inner.is_retriable_error(error)
    }

    // If ignore_unknown_values is set and the schema of the events inserted is wrong, bigquery
    // will still return a 200. We have to look in the response to determine if the schema was
    // wrong.
    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        match response.status() {
            StatusCode::OK => {
                let resp: InsertAllResponse = match serde_json::from_slice(response.body()) {
                    Ok(resp) => resp,
                    Err(e) => {
                        return RetryAction::DontRetry(format!(
                            "failed to deserialize response: {}",
                            e.to_string()
                        ))
                    }
                };

                if resp.contains_no_such_field_error() {
                    RetryAction::DontRetry("incorrect data".into())
                } else {
                    RetryAction::Successful
                }
            }
            _ => self.inner.should_retry_response(response),
        }
    }
}

async fn healthcheck(
    client: HttpClient,
    uri: Uri,
    creds: Option<GcpCredentials>,
) -> crate::Result<()> {
    let mut request = http::Request::get(uri.clone()).body(Body::empty())?;
    if let Some(creds) = creds.as_ref() {
        creds.apply(&mut request);
    }

    let not_found_error = HealthcheckError::HealthcheckTableNotFound {
        uri: uri.to_string(),
    }
    .into();

    let response = client.send(request).await?;
    healthcheck_response(creds, not_found_error)(response)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertAllRequestRows {
    /// [Optional] A unique ID for each row. BigQuery uses this property to detect duplicate insertion requests on a best-effort basis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_id: Option<String>,
    /// Represents a single JSON object.
    pub json: BoxedRawValue,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InsertAllRequest {
    /// [Optional] Accept rows that contain values that do not match the schema. The unknown values are ignored. Default is false, which treats unknown values as errors.
    ignore_unknown_values: bool,
    /// The resource type of the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    /// The rows to insert.
    rows: Vec<InsertAllRequestRows>,
    /// [Optional] Insert all valid rows of a request, even if invalid rows exist. The default value is false, which causes the entire request to fail if any invalid rows exist.
    skip_invalid_rows: bool,
    /// If specified, treats the destination table as a base template, and inserts the rows into an instance table named \"{destination}{templateSuffix}\". BigQuery will manage creation of the instance table, using the schema of the base template table. See https://cloud.google.com
    #[serde(skip_serializing_if = "Option::is_none")]
    template_suffix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertAllResponse {
    /// An array of errors for rows that were not inserted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_errors: Option<Vec<InsertErrors>>,
    /// The resource type of the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl InsertAllResponse {
    fn contains_no_such_field_error(&self) -> bool {
        match &self.insert_errors {
            None => return false,
            // iterate over errors, look for "no such field" error message
            Some(ref insert_all_errors) => {
                for insert_all_error in insert_all_errors {
                    if let Some(ref row_errors) = insert_all_error.errors {
                        for row_error in row_errors {
                            if let Some(msg) = &row_error.message {
                                if msg.contains(&"no such field") {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertErrors {
    /// Error information for the row indicated by the index property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ErrorProto>>,
    /// The index of the row that error applies to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorProto {
    /// Debugging information. This property is internal to Google and should not be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_info: Option<String>,
    /// Specifies where the error occurred, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// A human-readable description of the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// A short error code that summarizes the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<BigquerySinkConfig>();
    }
}

#[cfg(test)]
#[cfg(feature = "gcp-bigquery-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        config::{SinkConfig, SinkContext},
        test_util::trace_init,
    };
    use futures::{future, stream};
    use http::StatusCode;
    use std::{
        convert::Infallible,
        net::SocketAddr,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };
    use vector_core::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent};
    use warp::Filter;

    fn make_event() -> (Event, BatchStatusReceiver) {
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let mut event = LogEvent::from("raw log line").with_batch_notifier(&batch);
        event.insert("host", "example.com");
        (event.into(), receiver)
    }

    #[tokio::test]
    async fn happy_warp() {
        trace_init();

        let visited = Arc::new(AtomicBool::new(false));
        let routes = warp::any().and_then(move || {
            assert!(!visited.load(Ordering::SeqCst), "Should not retry request.");
            visited.store(true, Ordering::SeqCst);

            future::ok::<_, Infallible>(warp::reply::with_status("Code: 200", StatusCode::OK))
        });
        let server = warp::serve(routes).bind("0.0.0.0:8124".parse::<SocketAddr>().unwrap());
        tokio::spawn(server);

        let host = String::from("http://localhost:8124");

        let config = BigquerySinkConfig {
            endpoint: host.parse().unwrap(),
            project: "test".into(),
            dataset: "test".into(),
            table: "test".into(),
            ignore_unknown_values: false,
            skip_invalid_rows: false,
            template_suffix: None,
            batch: BatchConfig {
                max_events: Some(1),
                ..Default::default()
            },
            ..Default::default()
        };
        let (sink, _hc) = config
            .build(SinkContext::new_test())
            .await
            .expect("failed to build bigquery sink");

        let (input_event, mut receiver) = make_event();

        sink.run(stream::once(future::ready(input_event)))
            .await
            .expect("failed to run stream");

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
    }
}
