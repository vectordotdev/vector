mod models;
mod retry;

use self::models::{InsertAllRequest, InsertAllRequestRows};
use self::retry::{BigqueryRetryLogic, BigqueryServiceLogic};

use super::{healthcheck_response, GcpAuthConfig, GcpCredentials, Scope};
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    http::HttpClient,
    sinks::{
        util::{
            batch::{BatchConfig, BatchSettings},
            encoding::{EncodingConfigWithDefault, EncodingConfiguration},
            http::{BatchedHttpSink, HttpSink},
            BoxedRawValue, JsonArrayBuffer, TowerRequestConfig,
        },
        Healthcheck, UriParseError, VectorSink,
    },
    tls::{TlsOptions, TlsSettings},
};
use futures::{FutureExt, SinkExt};
use http::Uri;
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
            BigqueryServiceLogic::default(),
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
    async fn insert_events_warp() {
        trace_init();

        let visited = Arc::new(AtomicBool::new(false));
        let routes = warp::any().and_then(move || {
            assert!(!visited.load(Ordering::SeqCst), "Should not retry request.");
            visited.store(true, Ordering::SeqCst);

            future::ok::<_, Infallible>(warp::reply::with_status("Code: 200", StatusCode::OK))
        });
        let server = warp::serve(routes).bind("0.0.0.0:8123".parse::<SocketAddr>().unwrap());
        tokio::spawn(server);

        let host = String::from("http://localhost:8123");

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
            .expect("Sending events failed");

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
    }

    #[tokio::test]
    async fn incorrect_schema_warp() {
        trace_init();

        let visited = Arc::new(AtomicBool::new(false));
        let custom = warp::any().map(move || {
            assert!(!visited.load(Ordering::SeqCst), "Should not retry request.");
            visited.store(true, Ordering::SeqCst);
            http::Response::builder().body(
                r#"
{
    "insertErrors": [
        {
            "errors": [
                {
                    "debugInfo": "",
                    "location": "host",
                    "message": "no such field: host.",
                    "reason": "invalid"
                }
            ],
            "index": 0
        }
    ],
    "kind": "bigquery#tableDataInsertAllResponse"
}"#,
            )
        });

        let routes = warp::post().and(custom);
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

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Failed));
    }
}
