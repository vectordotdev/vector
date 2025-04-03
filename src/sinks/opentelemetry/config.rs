//! Configuration for the `opentelemetry` sink.

use crate::{
    http::{Auth, HttpClient, MaybeAuth},
    schema,
    sinks::{
        prelude::*,
        util::{
            http::{http_response_retry_logic, HttpService, RequestConfig},
            RealtimeSizeBasedDefaultBatchSettings, UriSerde,
        },
    },
};
use http::{Request, StatusCode};
use hyper::Body;
use vrl::value::Kind;

use super::{
    encoder::OpentelemetryEncoder, request_builder::OpentelemetryRequestBuilder,
    service::OpentelemetryServiceRequestBuilder, sink::OpentelemetrySink,
};

/// Configuration for the `opentelemetry` sink.
#[configurable_component(sink("opentelemetry", "Deliver logs to OpenTelemetry"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct OpenTelemetryConfig {
    /// The full URI to make HTTP requests to.
    ///
    /// This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
    #[configurable(metadata(docs::examples = "https://10.22.212.22:9000/v1"))]
    pub(super) uri: UriSerde,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub(super) encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) request: RequestConfig,

    #[configurable(derived)]
    pub(super) tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    pub auth: Option<Auth>,
}

impl_generate_config_from_default!(OpenTelemetryConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry")]
impl SinkConfig for OpenTelemetryConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let request_builder = OpentelemetryRequestBuilder {
            encoder: OpentelemetryEncoder::new(self.encoding.clone()),
        };

        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;

        // TODO: needs something better
        let log_endpoint = format!("{}v1/logs", self.uri);
        let trace_endpoint = format!("{}v1/traces", self.uri);
        let metric_endpoint = format!("{}v1/metrics", self.uri);

        let service_request_builder = OpentelemetryServiceRequestBuilder {
            auth: self.auth.choose_one(&self.uri.auth)?,
        };

        let service = HttpService::new(client.clone(), service_request_builder);

        let request_limits = self.request.tower.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, http_response_retry_logic())
            .service(service);

        let sink = OpentelemetrySink::new(
            service,
            batch_settings,
            request_builder,
            log_endpoint,
            trace_endpoint,
            metric_endpoint,
        );

        let healthcheck = match cx.healthcheck.uri {
            Some(healthcheck_uri) => {
                healthcheck(healthcheck_uri, self.auth.clone(), client.clone()).boxed()
            }
            None => future::ok(()).boxed(),
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        let requirement =
            schema::Requirement::empty().required_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

async fn healthcheck(uri: UriSerde, auth: Option<Auth>, client: HttpClient) -> crate::Result<()> {
    let auth = auth.choose_one(&uri.auth)?;
    let uri = uri.with_default_parts();
    let mut request = Request::head(&uri.uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}
