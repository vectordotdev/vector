//! The AppSignal sink
//!
//! This sink provides downstream support for `AppSignal` to collect logs and a subset of Vector
//! metric types. These events are sent to the `appsignal-endpoint.net` domain, which is part of
//! the `appsignal.com` infrastructure.
//!
//! Logs and metrics are stored on an per app basis and require an app-level Push API key.

#[cfg(all(test, feature = "appsignal-integration-tests"))]
mod integration_tests;

use bytes::Bytes;
use futures::{FutureExt, SinkExt};
use http::{header::AUTHORIZATION, Request, Uri};
use hyper::Body;
use serde_json::json;
use snafu::{ResultExt, Snafu};
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;

use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, DataType, Input, SinkConfig, SinkContext},
    event::Event,
    http::HttpClient,
    sinks::{
        util::{
            encoding::write_all,
            http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
            BatchConfig, BoxedRawValue, Compression, Compressor, JsonArrayBuffer,
            SinkBatchSettings, TowerRequestConfig,
        },
        BuildError,
    },
    tls::{TlsConfig, TlsSettings},
};

#[derive(Debug, Snafu)]
enum FinishError {
    #[snafu(display(
        "Failure occurred during writing to or finalizing the compressor: {}",
        source
    ))]
    CompressionFailed { source: std::io::Error },
}

/// Configuration for the `appsignal` sink.
#[configurable_component(sink("appsignal"))]
#[derive(Clone, Debug, Default)]
pub struct AppsignalSinkConfig {
    /// The URI for the AppSignal API to send data to.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "https://appsignal-endpoint.net"))]
    #[serde(default = "default_endpoint")]
    endpoint: String,

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
    tls: Option<TlsConfig>,

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
    "https://appsignal-endpoint.net".to_string()
}

#[derive(Clone, Copy, Debug, Default)]
struct AppsignalDefaultBatchSettings;

impl SinkBatchSettings for AppsignalDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(100);
    const MAX_BYTES: Option<usize> = Some(450_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

impl_generate_config_from_default!(AppsignalSinkConfig);

#[async_trait::async_trait]
impl SinkConfig for AppsignalSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let push_api_key = self.push_api_key.inner().to_string();
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = self.batch.into_batch_settings()?;

        let buffer = JsonArrayBuffer::new(batch_settings.size);

        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;

        let sink = BatchedHttpSink::new(
            self.clone(),
            buffer,
            request_settings,
            batch_settings.timeout,
            client.clone(),
        )
        .sink_map_err(|error| error!(message = "Fatal AppSignal sink error.", %error));

        let healthcheck = healthcheck(
            endpoint_uri(&self.endpoint, "vector/healthcheck")?,
            push_api_key,
            client,
        )
        .boxed();

        Ok((super::VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Metric | DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Encode logs and metrics for requests to the AppSignal API.
/// It will use a JSON format wrapping events in either "log" or "metric", based on the type of event.
pub struct AppsignalEventEncoder {
    transformer: Transformer,
}

impl HttpEventEncoder<serde_json::Value> for AppsignalEventEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<serde_json::Value> {
        self.transformer.transform(&mut event);

        match event {
            Event::Log(log) => Some(json!({ "log": log })),
            Event::Metric(metric) => Some(json!({ "metric": metric })),
            _ => panic!("The AppSignal sink does not support this type of event: {event:?}"),
        }
    }
}

#[async_trait::async_trait]
impl HttpSink for AppsignalSinkConfig {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;
    type Encoder = AppsignalEventEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        AppsignalEventEncoder {
            transformer: self.encoding.clone(),
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Bytes>> {
        let uri = endpoint_uri(&self.endpoint, "vector/events")?;
        let mut request = Request::post(uri).header(
            AUTHORIZATION,
            format!("Bearer {}", self.push_api_key.inner()),
        );

        let mut body = crate::serde::json::to_bytes(&events)?.freeze();
        if let Some(ce) = self.compression.content_encoding() {
            request = request.header("Content-Encoding", ce);
        }
        let mut compressor = Compressor::from(self.compression);
        write_all(&mut compressor, 0, &body)?;
        body = compressor.finish().context(CompressionFailedSnafu)?.into();
        request.body(body).map_err(Into::into)
    }
}

async fn healthcheck(uri: Uri, push_api_key: String, client: HttpClient) -> crate::Result<()> {
    let request = Request::get(uri).header(AUTHORIZATION, format!("Bearer {}", push_api_key));
    let response = client.send(request.body(Body::empty()).unwrap()).await?;

    match response.status() {
        status if status.is_success() => Ok(()),
        other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

fn endpoint_uri(endpoint: &str, path: &str) -> crate::Result<Uri> {
    let uri = if endpoint.ends_with('/') {
        format!("{endpoint}{path}")
    } else {
        format!("{endpoint}/{path}")
    };
    match uri.parse::<Uri>() {
        Ok(u) => Ok(u),
        Err(e) => Err(Box::new(BuildError::UriParseError { source: e })),
    }
}

#[cfg(test)]
mod test {
    use futures::{future::ready, stream};
    use serde::Deserialize;
    use vector_core::event::{Event, LogEvent};

    use crate::{
        config::{GenerateConfig, SinkConfig, SinkContext},
        test_util::{
            components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
            http::{always_200_response, spawn_blackhole_http_server},
        },
    };

    use super::{endpoint_uri, AppsignalSinkConfig};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AppsignalSinkConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

        let config = AppsignalSinkConfig::generate_config().to_string();
        let mut config =
            AppsignalSinkConfig::deserialize(toml::de::ValueDeserializer::new(&config))
                .expect("config should be valid");
        config.endpoint = mock_endpoint.to_string();

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let event = Event::Log(LogEvent::from("simple message"));
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
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
}
