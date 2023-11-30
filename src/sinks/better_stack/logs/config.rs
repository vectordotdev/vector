//! Configuration for the `better_stack_logs` sink.

use bytes::Bytes;
use futures::FutureExt;
use http::{Request, StatusCode, Uri};
use vector_lib::sensitive_string::SensitiveString;
use vrl::value::Kind;

use crate::{
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{
            http::{http_response_retry_logic, HttpService},
            BatchConfig, BoxedRawValue,
        },
    },
};

use super::{
    encoder::BetterStackLogsEncoder, request_builder::BetterStackLogsRequestBuilder,
    service::BetterStackLogsSvcRequestBuilder, sink::BetterStackLogsSink,
};

/// Configuration for the `better_stack_logs` sink.
#[configurable_component(sink("better_stack_logs", "Send logs to Better Stack."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BetterStackLogsConfig {
    // This endpoint is not user-configurable and only exists for testing purposes
    #[serde(skip, default = "default_endpoint")]
    pub(super) endpoint: String,

    /// The token key that is used to identify source and authenticate against Better Stack.
    #[configurable(metadata(docs::examples = "${BETTER_STACK_SOURCE_TOKEN}"))]
    #[configurable(metadata(docs::examples = "your-source-token"))]
    source_token: SensitiveString,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<BetterStackLogsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    encoding: Transformer,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_endpoint() -> String {
    "https://in.logs.betterstack.com".to_string()
}

#[derive(Clone, Copy, Debug, Default)]
struct BetterStackLogsDefaultBatchSettings;

impl SinkBatchSettings for BetterStackLogsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(100_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

impl GenerateConfig for BetterStackLogsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"source_token = "${BETTER_STACK_SOURCE_TOKEN}""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "better_stack_logs")]
impl SinkConfig for BetterStackLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let request_builder = BetterStackLogsRequestBuilder {
            encoder: BetterStackLogsEncoder {
                transformer: self.encoding.clone(),
            },
        };

        let uri = self.build_uri()?;

        let better_stack_logs_service_request_builder = BetterStackLogsSvcRequestBuilder {
            uri: uri.clone(),
            source_token: self.source_token.clone(),
        };

        let client = HttpClient::new(None, cx.proxy())?;

        let service = HttpService::new(client.clone(), better_stack_logs_service_request_builder);

        let request_limits = self.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, http_response_retry_logic())
            .service(service);

        let sink = BetterStackLogsSink::new(service, batch_settings, request_builder);

        let healthcheck = healthcheck(uri, self.source_token.clone(), client).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        let requirement = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl BetterStackLogsConfig {
    fn build_uri(&self) -> crate::Result<Uri> {
        let uri = &self.endpoint;
        uri.parse::<Uri>().map_err(Into::into)
    }
}

async fn healthcheck(uri: Uri, source_token: SensitiveString, client: HttpClient) -> crate::Result<()> {
    let request = Request::post(uri).header("Authorization", format!("Bearer {}", source_token.inner()));
    let body = crate::serde::json::to_bytes(&Vec::<BoxedRawValue>::new())
        .unwrap()
        .freeze();
    let req: Request<Bytes> = request.body(body)?;
    let req = req.map(hyper::Body::from);

    let res = client.send(req).await?;

    let status = res.status();
    let body = hyper::body::to_bytes(res.into_body()).await?;

    if status == StatusCode::BAD_REQUEST {
        Ok(())
    } else if status == StatusCode::UNAUTHORIZED {
        let json: serde_json::Value = serde_json::from_slice(&body[..])?;

        let message = if let Some(s) = json
            .as_object()
            .and_then(|o| o.get("error"))
            .and_then(|s| s.as_str())
        {
            s.to_string()
        } else {
            "Token is not valid, 401 returned.".to_string()
        };

        Err(message.into())
    } else {
        let body = String::from_utf8_lossy(&body[..]);

        Err(format!(
            "Server returned unexpected error status: {} body: {}",
            status, body
        )
        .into())
    }
}
