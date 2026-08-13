use futures::FutureExt;
use http::{Request, Uri, header::AUTHORIZATION};
use hyper::Body;
use tower::ServiceBuilder;
use vector_lib::{
    config::{AcknowledgementsConfig, DataType, Input, proxy::ProxyConfig},
    configurable::configurable_component,
    sensitive_string::SensitiveString,
    stream::BatcherSettings,
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

use super::{
    service::{AppsignalResponse, AppsignalService},
    sink::AppsignalSink,
};
use crate::{
    codecs::Transformer,
    config::ValidatedSink,
    http::HttpClient,
    sinks::{
        BuildError, Healthcheck, HealthcheckError, VectorSink,
        prelude::{SinkConfig, SinkContext},
        util::{
            BatchConfig, Compression, ServiceBuilderExt, SinkBatchSettings, TowerRequestConfig,
            http::{HttpStatusRetryLogic, RetryStrategy},
        },
    },
};

/// Configuration for the `appsignal` sink.
#[configurable_component(sink("appsignal", "Deliver log and metric event data to AppSignal."))]
#[derive(Clone, Debug, Default)]
pub(super) struct AppsignalConfig {
    /// The URI for the AppSignal API to send data to.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "https://appsignal-endpoint.net"))]
    #[serde(default = "default_endpoint")]
    pub(super) endpoint: String,

    /// A valid app-level AppSignal Push API key.
    #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
    #[configurable(metadata(docs::examples = "${APPSIGNAL_PUSH_API_KEY}"))]
    push_api_key: SensitiveString,

    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<AppsignalDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    encoding: Transformer,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    retry_strategy: RetryStrategy,
}

pub(super) fn default_endpoint() -> String {
    "https://appsignal-endpoint.net".to_string()
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AppsignalDefaultBatchSettings;

impl SinkBatchSettings for AppsignalDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(100);
    const MAX_BYTES: Option<usize> = Some(450_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

impl AppsignalConfig {
    pub(super) fn build_client(
        &self,
        proxy: &ProxyConfig,
        tls: &MaybeTlsSettings,
    ) -> crate::Result<HttpClient> {
        let client = HttpClient::new(tls.clone(), proxy)?;
        Ok(client)
    }

    pub(super) fn build_sink(
        &self,
        http_client: HttpClient,
        batch_settings: BatcherSettings,
        endpoint: Uri,
    ) -> crate::Result<VectorSink> {
        let push_api_key = self.push_api_key.clone();
        let compression = self.compression;
        let service = AppsignalService::new(http_client, endpoint, push_api_key, compression);

        let request_opts = self.request;
        let request_settings = request_opts.into_settings();
        let retry_logic = HttpStatusRetryLogic::new(
            |req: &AppsignalResponse| req.http_status,
            self.retry_strategy.clone(),
        );

        let service = ServiceBuilder::new()
            .settings(request_settings, retry_logic)
            .service(service);

        let transformer = self.encoding.clone();
        let sink = AppsignalSink {
            service,
            compression,
            transformer,
            batch_settings,
        };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}

impl_generate_config_from_default!(AppsignalConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "appsignal")]
impl SinkConfig for AppsignalConfig {
    fn input(&self) -> Input {
        Input::new(DataType::Metric | DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedAppsignal {
    batch_settings: BatcherSettings,
    endpoint: Uri,
    healthcheck_endpoint: Uri,
}

#[async_trait::async_trait]
impl ValidatedSink for AppsignalConfig {
    type Validated = ValidatedAppsignal;

    fn validate(&self) -> crate::Result<ValidatedAppsignal> {
        let batch_settings = self.batch.into_batcher_settings()?;
        let endpoint = endpoint_uri(&self.endpoint, "vector/events")?;
        let healthcheck_endpoint = endpoint_uri(&self.endpoint, "vector/healthcheck")?;

        Ok(ValidatedAppsignal {
            batch_settings,
            endpoint,
            healthcheck_endpoint,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedAppsignal,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedAppsignal {
            batch_settings,
            endpoint,
            healthcheck_endpoint,
        } = validated.clone();

        // TLS settings may read certificate files from disk, so they are
        // resolved at build time rather than during pure validation.
        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), false)?;
        let client = self.build_client(cx.proxy(), &tls)?;
        let healthcheck = healthcheck(
            healthcheck_endpoint,
            self.push_api_key.inner().to_string(),
            client.clone(),
        )
        .boxed();
        let sink = self.build_sink(client, batch_settings, endpoint)?;

        Ok((sink, healthcheck))
    }
}

async fn healthcheck(uri: Uri, push_api_key: String, client: HttpClient) -> crate::Result<()> {
    let request = Request::get(uri).header(AUTHORIZATION, format!("Bearer {push_api_key}"));
    let response = client.send(request.body(Body::empty()).unwrap()).await?;

    match response.status() {
        status if status.is_success() => Ok(()),
        other => Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

pub fn endpoint_uri(endpoint: &str, path: &str) -> crate::Result<Uri> {
    let uri = if endpoint.ends_with('/') {
        format!("{endpoint}{path}")
    } else {
        format!("{endpoint}/{path}")
    };
    let uri = match uri.parse::<Uri>() {
        Ok(u) => u,
        Err(e) => return Err(Box::new(BuildError::UriParseError { source: e })),
    };
    // Reject endpoints that parse but lack a scheme or authority (e.g.
    // `appsignal-endpoint.net`). `http::Uri` accepts them, but the request
    // path hands them to the HTTP client as a non-absolute target that
    // deterministically fails at build time.
    if uri.scheme().is_none() || uri.authority().is_none() {
        return Err(format!(
            "endpoint must include a scheme and host, e.g. `https://appsignal-endpoint.net`; got `{endpoint}`"
        )
        .into());
    }
    Ok(uri)
}

#[cfg(test)]
mod test {
    use super::{AppsignalConfig, endpoint_uri};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AppsignalConfig>();
    }

    #[test]
    fn endpoint_uri_with_path() {
        let uri = endpoint_uri("https://appsignal-endpoint.net", "vector/events");
        assert_eq!(
            uri.expect("Not a valid URI").to_string(),
            "https://appsignal-endpoint.net/vector/events"
        );
    }

    #[test]
    fn endpoint_uri_with_trailing_slash() {
        let uri = endpoint_uri("https://appsignal-endpoint.net/", "vector/events");
        assert_eq!(
            uri.expect("Not a valid URI").to_string(),
            "https://appsignal-endpoint.net/vector/events"
        );
    }

    #[test]
    fn validate_rejects_relative_endpoint() {
        use crate::config::ValidatedSink;

        let config = AppsignalConfig {
            endpoint: "/vector".to_string(),
            ..Default::default()
        };
        let err = config
            .validate()
            .expect_err("a relative endpoint must be rejected");
        assert!(
            err.to_string().contains("scheme and host"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_produces_usable_values() {
        use crate::config::ValidatedSink;

        let config = AppsignalConfig {
            endpoint: "https://appsignal-endpoint.net".to_string(),
            ..Default::default()
        };
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(
            validated.endpoint.to_string(),
            "https://appsignal-endpoint.net/vector/events"
        );
        assert_eq!(
            validated.healthcheck_endpoint.to_string(),
            "https://appsignal-endpoint.net/vector/healthcheck"
        );
    }
}
