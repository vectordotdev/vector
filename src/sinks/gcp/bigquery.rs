use super::{healthcheck_response, GcpAuthConfig, GcpCredentials, Scope};
use crate::{
    event::Event,
    sinks::{
        util::{
            http::{BatchedHttpSink, HttpClient, HttpSink},
            BatchBytesConfig, BoxedRawValue, JsonArrayBuffer, TowerRequestConfig,
        },
        Healthcheck, RouterSink, UriParseError,
    },
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures01::Future;
use http::{Method, Uri};
use hyper::{Body, Request};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::{ResultExt, Snafu};
use tower::Service;

const NAME: &str = "gcp_bigquery";

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Configured topic not found"))]
    TopicNotFound,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
struct BigQueryConfig {
    project: String,
    dataset: String,
    table: String,

    pub emulator_host: Option<String>,
    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,

    pub tls: Option<TlsOptions>,
}

inventory::submit! {
    SinkDescription::new::<BigQueryConfig>(NAME)
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        rate_limit_num: Some(100),
        rate_limit_duration_secs: Some(1),
        ..Default::default()
    };
}

#[typetag::serde(name = "gcp_bigquery")]
impl SinkConfig for BigQueryConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(RouterSink, Healthcheck)> {
        let sink = BigQuerySink::from_config(self)?;
        let batch_settings = self.batch.unwrap_or(bytesize::mib(10u64), 1);
        let request_settings = self.request.unwrap_with(&REQUEST_DEFAULTS);

        let healthcheck = sink.healthcheck(&cx)?;

        let service = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::default(),
            request_settings,
            batch_settings,
            None,
            &cx,
        );

        Ok((Box::new(service), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        NAME
    }
}

struct BigQuerySink {
    creds: GcpCredentials,
    tls: TlsSettings,
    uri_base: String,
}

impl BigQuerySink {
    fn from_config(config: &BigQueryConfig) -> crate::Result<Self> {
        let creds = config.auth.make_required_credentials(Scope::BigQuery)?;

        let base = match config.emulator_host.as_ref() {
            Some(host) => format!("http://{}", host),
            None => "https://bigquery.googleapis.com".into(),
        };
        let uri_base = format!(
            "{}/bigquery/v2/projects/{}/datasets/{}/tables/{}",
            base, config.project, config.dataset, config.table
        );
        uri_base.parse::<Uri>().context(UriParseError)?;

        let tls = TlsSettings::from_options(&config.tls)?;

        Ok(Self {
            creds,
            tls,
            uri_base,
        })
    }

    fn healthcheck(&self, cx: &SinkContext) -> crate::Result<Healthcheck> {
        let uri = self.uri("");
        let mut request = Request::get(uri).body(Body::empty()).unwrap();
        self.creds.apply(&mut request);

        let mut client = HttpClient::new(cx.resolver(), self.tls.clone())?;
        let healthcheck = client
            .call(request)
            .map_err(Into::into)
            .and_then(healthcheck_response(
                Some(self.creds.clone()),
                HealthcheckError::TopicNotFound.into(),
            ));
        Ok(Box::new(healthcheck))
    }

    fn uri(&self, suffix: &str) -> Uri {
        format!("{}{}", self.uri_base, suffix)
            .parse::<Uri>()
            .unwrap()
    }
}

impl HttpSink for BigQuerySink {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        Some(json!({
            "json": event.into_log(),
        }))
    }

    fn build_request(&self, events: Self::Output) -> http::Request<Vec<u8>> {
        let body = serde_json::to_vec(&json!({
            "rows": events,
        }))
        .unwrap();
        let mut builder = hyper::Request::builder();
        builder.method(Method::POST);
        builder.uri(self.uri("/insertAll"));
        builder.header("Content-Type", "application/json");

        let mut request = builder.body(body).unwrap();
        self.creds.apply(&mut request);

        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::runtime;

    #[test]
    fn fails_missing_creds() {
        let config: BigQueryConfig = toml::from_str(
            r#"
               project = "project"
               dataset = "dataset"
               table   = "table"
        "#,
        )
        .unwrap();
        if config
            .build(SinkContext::new_test(runtime().executor()))
            .is_ok()
        {
            panic!("config.build failed to error");
        }
    }
}
