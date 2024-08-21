use base64::prelude::{Engine as _, BASE64_STANDARD};
use bytes::{Bytes, BytesMut};
use futures::{FutureExt, SinkExt};
use http::{Request, Uri};
use hyper::Body;
use indoc::indoc;
use serde_json::{json, Value};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Encoder as _;
use vector_lib::configurable::configurable_component;

use crate::{
    codecs::{Encoder, EncodingConfig, Transformer},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    event::Event,
    gcp::{GcpAuthConfig, GcpAuthenticator, Scope, PUBSUB_URL},
    http::HttpClient,
    sinks::{
        gcs_common::config::healthcheck_response,
        util::{
            http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
            BatchConfig, BoxedRawValue, JsonArrayBuffer, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, UriParseSnafu, VectorSink,
    },
    tls::{TlsConfig, TlsSettings},
};

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Configured topic not found"))]
    TopicNotFound,
}

// 10MB maximum message size: https://cloud.google.com/pubsub/quotas#resource_limits
const MAX_BATCH_PAYLOAD_SIZE: usize = 10_000_000;

#[derive(Clone, Copy, Debug, Default)]
pub struct PubsubDefaultBatchSettings;

impl SinkBatchSettings for PubsubDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1000);
    const MAX_BYTES: Option<usize> = Some(10_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `gcp_pubsub` sink.
#[configurable_component(sink(
    "gcp_pubsub",
    "Publish observability events to GCP's Pub/Sub messaging system."
))]
#[derive(Clone, Debug)]
pub struct PubsubConfig {
    /// The project name to which to publish events.
    #[configurable(metadata(docs::examples = "vector-123456"))]
    pub project: String,

    /// The topic within the project to which to publish events.
    #[configurable(metadata(docs::examples = "this-is-a-topic"))]
    pub topic: String,

    /// The endpoint to which to publish events.
    ///
    /// The scheme (`http` or `https`) must be specified. No path should be included since the paths defined
    /// by the [`GCP Pub/Sub`][pubsub_api] API are used.
    ///
    /// The trailing slash `/` must not be included.
    ///
    /// [pubsub_api]: https://cloud.google.com/pubsub/docs/reference/rest
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "https://us-central1-pubsub.googleapis.com"))]
    pub endpoint: String,

    #[serde(default, flatten)]
    pub auth: GcpAuthConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<PubsubDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_endpoint() -> String {
    PUBSUB_URL.to_string()
}

impl GenerateConfig for PubsubConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            project = "my-project"
            topic = "my-topic"
            encoding.codec = "json"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_pubsub")]
impl SinkConfig for PubsubConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = PubsubSink::from_config(self).await?;
        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_BATCH_PAYLOAD_SIZE)?
            .into_batch_settings()?;
        let request_settings = self.request.into_settings();
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;

        let healthcheck = healthcheck(client.clone(), sink.uri("")?, sink.auth.clone()).boxed();
        sink.auth.spawn_regenerate_token();

        let sink = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::new(batch_settings.size),
            request_settings,
            batch_settings.timeout,
            client,
        )
        .sink_map_err(|error| error!(message = "Fatal gcp_pubsub sink error.", %error));

        #[allow(deprecated)]
        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct PubsubSink {
    auth: GcpAuthenticator,
    uri_base: String,
    transformer: Transformer,
    encoder: Encoder<()>,
}

impl PubsubSink {
    async fn from_config(config: &PubsubConfig) -> crate::Result<Self> {
        // We only need to load the credentials if we are not targeting an emulator.
        let auth = config.auth.build(Scope::PubSub).await?;

        let uri_base = format!(
            "{}/v1/projects/{}/topics/{}",
            config.endpoint, config.project, config.topic,
        );

        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(Self {
            auth,
            uri_base,
            transformer,
            encoder,
        })
    }

    fn uri(&self, suffix: &str) -> crate::Result<Uri> {
        let uri = format!("{}{}", self.uri_base, suffix);
        let mut uri = uri.parse::<Uri>().context(UriParseSnafu)?;
        self.auth.apply_uri(&mut uri);
        Ok(uri)
    }
}

struct PubSubSinkEventEncoder {
    transformer: Transformer,
    encoder: Encoder<()>,
}

impl HttpEventEncoder<Value> for PubSubSinkEventEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<Value> {
        self.transformer.transform(&mut event);
        let mut bytes = BytesMut::new();
        // Errors are handled by `Encoder`.
        self.encoder.encode(event, &mut bytes).ok()?;
        // Each event needs to be base64 encoded, and put into a JSON object
        // as the `data` item.
        Some(json!({ "data": BASE64_STANDARD.encode(&bytes) }))
    }
}

impl HttpSink for PubsubSink {
    type Input = Value;
    type Output = Vec<BoxedRawValue>;
    type Encoder = PubSubSinkEventEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        PubSubSinkEventEncoder {
            transformer: self.transformer.clone(),
            encoder: self.encoder.clone(),
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Bytes>> {
        let body = json!({ "messages": events });
        let body = crate::serde::json::to_bytes(&body).unwrap().freeze();

        let uri = self.uri(":publish").unwrap();
        let builder = Request::post(uri).header("Content-Type", "application/json");

        let mut request = builder.body(body).unwrap();
        self.auth.apply(&mut request);

        Ok(request)
    }
}

async fn healthcheck(client: HttpClient, uri: Uri, auth: GcpAuthenticator) -> crate::Result<()> {
    let mut request = Request::get(uri).body(Body::empty()).unwrap();
    auth.apply(&mut request);

    let response = client.send(request).await?;
    healthcheck_response(response, HealthcheckError::TopicNotFound.into())
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PubsubConfig>();
    }

    #[tokio::test]
    async fn fails_missing_creds() {
        let config: PubsubConfig = toml::from_str(indoc! {r#"
                project = "project"
                topic = "topic"
                encoding.codec = "json"
            "#})
        .unwrap();
        if config.build(SinkContext::default()).await.is_ok() {
            panic!("config.build failed to error");
        }
    }
}

#[cfg(all(test, feature = "gcp-integration-tests"))]
mod integration_tests {
    use reqwest::{Client, Method, Response};
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};
    use vector_lib::codecs::JsonSerializerConfig;
    use vector_lib::event::{BatchNotifier, BatchStatus};

    use super::*;
    use crate::gcp;
    use crate::test_util::components::{run_and_assert_sink_error, COMPONENT_ERROR_TAGS};
    use crate::test_util::{
        components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
        random_events_with_stream, random_string, trace_init,
    };

    const PROJECT: &str = "testproject";

    fn config(topic: &str) -> PubsubConfig {
        PubsubConfig {
            project: PROJECT.into(),
            topic: topic.into(),
            endpoint: gcp::PUBSUB_ADDRESS.clone(),
            auth: GcpAuthConfig {
                skip_authentication: true,
                ..Default::default()
            },
            batch: Default::default(),
            request: Default::default(),
            encoding: JsonSerializerConfig::default().into(),
            tls: Default::default(),
            acknowledgements: Default::default(),
        }
    }

    async fn config_build(topic: &str) -> (VectorSink, crate::sinks::Healthcheck) {
        let cx = SinkContext::default();
        config(topic).build(cx).await.expect("Building sink failed")
    }

    #[tokio::test]
    async fn publish_events() {
        trace_init();

        let (topic, subscription) = create_topic_subscription().await;
        let (sink, healthcheck) = config_build(&topic).await;

        healthcheck.await.expect("Health check failed");

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let response = pull_messages(&subscription, 1000).await;
        let messages = response
            .receivedMessages
            .as_ref()
            .expect("Response is missing messages");
        assert_eq!(input.len(), messages.len());
        for i in 0..input.len() {
            let data = messages[i].message.decode_data();
            let data = serde_json::to_value(data).unwrap();
            let expected =
                serde_json::to_value(input[i].as_log().all_event_fields().unwrap()).unwrap();
            assert_eq!(data, expected);
        }
    }

    #[tokio::test]
    async fn publish_events_broken_topic() {
        trace_init();

        let (topic, _subscription) = create_topic_subscription().await;
        let (sink, _healthcheck) = config_build(&format!("BREAK{}BREAK", topic)).await;
        // Explicitly skip healthcheck

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_error(sink, events, &COMPONENT_ERROR_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
    }

    #[tokio::test]
    async fn checks_for_valid_topic() {
        trace_init();

        let (topic, _subscription) = create_topic_subscription().await;
        let topic = format!("BAD{}", topic);
        let (_sink, healthcheck) = config_build(&topic).await;
        healthcheck.await.expect_err("Health check did not fail");
    }

    async fn create_topic_subscription() -> (String, String) {
        let topic = format!("topic-{}", random_string(10));
        let subscription = format!("subscription-{}", random_string(10));
        request(Method::PUT, &format!("topics/{}", topic), json!({}))
            .await
            .json::<Value>()
            .await
            .expect("Creating new topic failed");
        request(
            Method::PUT,
            &format!("subscriptions/{}", subscription),
            json!({ "topic": format!("projects/{}/topics/{}", PROJECT, topic) }),
        )
        .await
        .json::<Value>()
        .await
        .expect("Creating new subscription failed");
        (topic, subscription)
    }

    async fn request(method: Method, path: &str, json: Value) -> Response {
        let url = format!("{}/v1/projects/{}/{}", *gcp::PUBSUB_ADDRESS, PROJECT, path);
        Client::new()
            .request(method.clone(), &url)
            .json(&json)
            .send()
            .await
            .unwrap_or_else(|_| panic!("Sending {} request to {} failed", method, url))
    }

    async fn pull_messages(subscription: &str, count: usize) -> PullResponse {
        request(
            Method::POST,
            &format!("subscriptions/{}:pull", subscription),
            json!({
                "returnImmediately": true,
                "maxMessages": count
            }),
        )
        .await
        .json::<PullResponse>()
        .await
        .expect("Extracting pull data failed")
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct PullResponse {
        receivedMessages: Option<Vec<PullMessageOuter>>,
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    #[allow(dead_code)] // deserialize all fields
    struct PullMessageOuter {
        ackId: String,
        message: PullMessage,
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    #[allow(dead_code)] // deserialize all fields
    struct PullMessage {
        data: String,
        messageId: String,
        publishTime: String,
    }

    impl PullMessage {
        fn decode_data(&self) -> TestMessage {
            let data = BASE64_STANDARD
                .decode(&self.data)
                .expect("Invalid base64 data");
            let data = String::from_utf8_lossy(&data);
            serde_json::from_str(&data).expect("Invalid message structure")
        }
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct TestMessage {
        timestamp: String,
        message: String,
    }
}
