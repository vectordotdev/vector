#![expect(
    clippy::let_underscore_must_use,
    reason = "derivative's Debug derive with ignored fields expands to a must_use let binding"
)]

use derivative::Derivative;
use futures::FutureExt;
use http::{HeaderValue, Request, header::AUTHORIZATION};
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
        Healthcheck, HealthcheckError, VectorSink,
        prelude::{SinkConfig, SinkContext},
        util::{
            BatchConfig, Compression, HttpEndpoint, ServiceBuilderExt, SinkBatchSettings,
            TowerRequestConfig,
            http::{HttpStatusRetryLogic, RetryStrategy},
        },
    },
};

/// Configuration for the `appsignal` sink.
#[configurable_component(sink("appsignal", "Deliver log and metric event data to AppSignal."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub(super) struct AppsignalConfig {
    /// The URI for the AppSignal API to send data to.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "https://appsignal-endpoint.net"))]
    #[derivative(Default(value = "default_endpoint()"))]
    #[serde(default = "default_endpoint")]
    pub(super) endpoint: HttpEndpoint,

    /// A valid app-level AppSignal Push API key.
    #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
    #[configurable(metadata(docs::examples = "${APPSIGNAL_PUSH_API_KEY}"))]
    push_api_key: SensitiveString,

    #[serde(default = "Compression::gzip_default")]
    compression: Compression,

    #[serde(default)]
    batch: BatchConfig<AppsignalDefaultBatchSettings>,

    #[serde(default)]
    request: TowerRequestConfig,

    tls: Option<TlsEnableableConfig>,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    encoding: Transformer,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[serde(default)]
    retry_strategy: RetryStrategy,
}

pub(super) fn default_endpoint() -> HttpEndpoint {
    HttpEndpoint::parse("https://appsignal-endpoint.net").unwrap()
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
        endpoint: HttpEndpoint,
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

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ValidatedAppsignal {
    batch_settings: BatcherSettings,
    endpoint: HttpEndpoint,
    healthcheck_endpoint: HttpEndpoint,
    // Omitted: `authorization` embeds the push API key.
    #[derivative(Debug = "ignore")]
    authorization: HeaderValue,
}

#[async_trait::async_trait]
impl ValidatedSink for AppsignalConfig {
    type Validated = ValidatedAppsignal;

    fn validate(&self) -> crate::Result<ValidatedAppsignal> {
        let batch_settings = self.batch.into_batcher_settings()?;
        let endpoint = endpoint_uri(&self.endpoint, "vector/events")?;
        let healthcheck_endpoint = endpoint_uri(&self.endpoint, "vector/healthcheck")?;
        let authorization = HeaderValue::from_str(&format!("Bearer {}", self.push_api_key.inner()))
            .map_err(|e| format!("invalid push_api_key: {e}"))?;

        Ok(ValidatedAppsignal {
            batch_settings,
            endpoint,
            healthcheck_endpoint,
            authorization,
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
            authorization,
        } = validated.clone();

        // TLS settings may read certificate files from disk, so they are
        // resolved at build time rather than during pure validation.
        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), false)?;
        let client = self.build_client(cx.proxy(), &tls)?;
        let healthcheck = healthcheck(healthcheck_endpoint, authorization, client.clone()).boxed();
        let sink = self.build_sink(client, batch_settings, endpoint)?;

        Ok((sink, healthcheck))
    }
}

async fn healthcheck(
    uri: HttpEndpoint,
    authorization: HeaderValue,
    client: HttpClient,
) -> crate::Result<()> {
    let request = Request::get(uri.as_uri()).header(AUTHORIZATION, authorization);
    let response = client.send(request.body(Body::empty()).unwrap()).await?;

    match response.status() {
        status if status.is_success() => Ok(()),
        other => Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

pub fn endpoint_uri(endpoint: &HttpEndpoint, path: &str) -> crate::Result<HttpEndpoint> {
    Ok(endpoint.append_path(path)?)
}

#[cfg(test)]
mod test {
    use super::{AppsignalConfig, HttpEndpoint, endpoint_uri};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AppsignalConfig>();
    }

    #[test]
    fn endpoint_uri_with_path() {
        let uri = endpoint_uri(
            &HttpEndpoint::parse("https://appsignal-endpoint.net").unwrap(),
            "vector/events",
        );
        assert_eq!(
            uri.expect("Not a valid URI").to_string(),
            "https://appsignal-endpoint.net/vector/events"
        );
    }

    #[test]
    fn endpoint_uri_with_trailing_slash() {
        let uri = endpoint_uri(
            &HttpEndpoint::parse("https://appsignal-endpoint.net/").unwrap(),
            "vector/events",
        );
        assert_eq!(
            uri.expect("Not a valid URI").to_string(),
            "https://appsignal-endpoint.net/vector/events"
        );
    }

    #[test]
    fn validate_produces_usable_values() {
        use crate::config::ValidatedSink;

        let config = AppsignalConfig {
            endpoint: HttpEndpoint::parse("https://appsignal-endpoint.net").unwrap(),
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

    #[test]
    fn validate_fails_on_invalid_push_api_key() {
        use crate::config::ValidatedSink;

        let config = AppsignalConfig {
            endpoint: HttpEndpoint::parse("https://appsignal-endpoint.net").unwrap(),
            push_api_key: "key\nwith_newline".to_string().into(),
            ..Default::default()
        };
        assert!(
            config.validate().is_err(),
            "an API key with an invalid header byte must fail validation"
        );
    }
}
