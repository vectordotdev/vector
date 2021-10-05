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

const NAME: &str = "gcp_bigquery";
const BASE_URL: &str = "https://bigquery.googleapis.com/bigquery/v2/";

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

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct BigquerySinkConfig {
    project: String,
    dataset: String,
    table: String,
    #[serde(default)]
    request: TowerRequestConfig,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(flatten)]
    auth: GcpAuthConfig,
    #[serde(default)]
    pub batch: BatchConfig,
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

        let sink = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::new(batch_settings.size),
            request_settings,
            batch_settings.timeout,
            client,
            cx.acker(),
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
    api_key: Option<String>,
    creds: Option<GcpCredentials>,
    uri_base: String,
    encoding: EncodingConfigWithDefault<Encoding>,
}

impl BigquerySink {
    async fn from_config(config: &BigquerySinkConfig) -> crate::Result<Self> {
        let creds = config.auth.make_credentials(Scope::BigQuery).await?;
        let uri_base = format!(
            "{}projects/{}/datasets/{}/tables/{}",
            BASE_URL, config.project, config.dataset, config.table
        );

        Ok(BigquerySink {
            api_key: config.auth.api_key.clone(),
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
        let insert_request_rows = events
            .into_iter()
            .map(|v| TableDataInsertAllRequestRows {
                insert_id: None,
                json: v,
            })
            .collect();
        let insert_request = TableDataInsertAllRequest {
            ignore_unknown_values: true,
            kind: None,
            rows: insert_request_rows,
            skip_invalid_rows: false,
            template_suffix: None,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataInsertAllRequestRows {
    /// [Optional] A unique ID for each row. BigQuery uses this property to detect duplicate insertion requests on a best-effort basis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_id: Option<String>,
    /// Represents a single JSON object.
    pub json: BoxedRawValue,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TableDataInsertAllRequest {
    /// [Optional] Accept rows that contain values that do not match the schema. The unknown values are ignored. Default is false, which treats unknown values as errors.
    ignore_unknown_values: bool,
    /// The resource type of the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    /// The rows to insert.
    rows: Vec<TableDataInsertAllRequestRows>,
    /// [Optional] Insert all valid rows of a request, even if invalid rows exist. The default value is false, which causes the entire request to fail if any invalid rows exist.
    skip_invalid_rows: bool,
    /// If specified, treats the destination table as a base template, and inserts the rows into an instance table named \"{destination}{templateSuffix}\". BigQuery will manage creation of the instance table, using the schema of the base template table. See https://cloud.google.com
    #[serde(skip_serializing_if = "Option::is_none")]
    template_suffix: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<BigquerySinkConfig>();
    }
}
