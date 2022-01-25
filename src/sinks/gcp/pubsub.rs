use std::num::NonZeroU64;

use futures::{FutureExt, SinkExt};
use http::{Request, Uri};
use hyper::Body;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use snafu::{ResultExt, Snafu};

use super::{GcpAuthConfig, GcpCredentials, Scope};
use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    http::HttpClient,
    sinks::{
        gcs_common::config::healthcheck_response,
        util::{
            encoding::{EncodingConfigWithDefault, EncodingConfiguration},
            http::{BatchedHttpSink, HttpSink},
            BatchConfig, BoxedRawValue, JsonArrayBuffer, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, UriParseSnafu, VectorSink,
    },
    tls::{TlsOptions, TlsSettings},
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
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct PubsubConfig {
    pub project: String,
    pub topic: String,
    pub endpoint: Option<String>,
    #[serde(default = "default_skip_authentication")]
    pub skip_authentication: bool,
    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    #[serde(default)]
    pub batch: BatchConfig<PubsubDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,

    pub tls: Option<TlsOptions>,
}

const fn default_skip_authentication() -> bool {
    false
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

inventory::submit! {
    SinkDescription::new::<PubsubConfig>("gcp_pubsub")
}

impl_generate_config_from_default!(PubsubConfig);

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
        let request_settings = self.request.unwrap_with(&Default::default());
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;

        let healthcheck = healthcheck(client.clone(), sink.uri("")?, sink.creds.clone()).boxed();

        let sink = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::new(batch_settings.size),
            request_settings,
            batch_settings.timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal gcp_pubsub sink error.", %error));

        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "gcp_pubsub"
    }
}

struct PubsubSink {
    api_key: Option<String>,
    creds: Option<GcpCredentials>,
    uri_base: String,
    encoding: EncodingConfigWithDefault<Encoding>,
}

impl PubsubSink {
    async fn from_config(config: &PubsubConfig) -> crate::Result<Self> {
        // We only need to load the credentials if we are not targeting an emulator.
        let creds = if config.skip_authentication {
            None
        } else {
            config.auth.make_credentials(Scope::PubSub).await?
        };

        let uri_base = match config.endpoint.as_ref() {
            Some(host) => host.to_string(),
            None => "https://pubsub.googleapis.com".into(),
        };
        let uri_base = format!(
            "{}/v1/projects/{}/topics/{}",
            uri_base, config.project, config.topic,
        );

        Ok(Self {
            api_key: config.auth.api_key.clone(),
            encoding: config.encoding.clone(),
            creds,
            uri_base,
        })
    }

    fn uri(&self, suffix: &str) -> crate::Result<Uri> {
        let mut uri = format!("{}{}", self.uri_base, suffix);
        if let Some(key) = &self.api_key {
            uri = format!("{}?key={}", uri, key);
        }
        uri.parse::<Uri>()
            .context(UriParseSnafu)
            .map_err(Into::into)
    }
}

#[async_trait::async_trait]
impl HttpSink for PubsubSink {
    type Input = Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);
        // Each event needs to be base64 encoded, and put into a JSON object
        // as the `data` item.
        let log = event.into_log();
        let json = serde_json::to_string(&log).unwrap();
        Some(json!({ "data": base64::encode(&json) }))
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        let body = json!({ "messages": events });
        let body = serde_json::to_vec(&body).unwrap();

        let uri = self.uri(":publish").unwrap();
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
    let mut request = Request::get(uri).body(Body::empty()).unwrap();
    if let Some(creds) = creds.as_ref() {
        creds.apply(&mut request);
    }

    let response = client.send(request).await?;
    healthcheck_response(creds, HealthcheckError::TopicNotFound.into())(response)
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
            "#})
        .unwrap();
        if config.build(SinkContext::new_test()).await.is_ok() {
            panic!("config.build failed to error");
        }
    }
}

#[cfg(test)]
#[cfg(feature = "gcp-pubsub-integration-tests")]
mod integration_tests {
    use reqwest::{Client, Method, Response};
    use serde_json::{json, Value};
    use vector_core::event::{BatchNotifier, BatchStatus};

    use super::*;
    use crate::test_util::{
        components::{self, HTTP_SINK_TAGS},
        random_events_with_stream, random_string, trace_init,
    };

    fn emulator_address() -> String {
        std::env::var("EMULATOR_ADDRESS").unwrap_or_else(|_| "http://localhost:8681".into())
    }

    const PROJECT: &str = "testproject";

    fn config(topic: &str) -> PubsubConfig {
        PubsubConfig {
            endpoint: Some(emulator_address()),
            skip_authentication: true,
            project: PROJECT.into(),
            topic: topic.into(),
            ..Default::default()
        }
    }

    async fn config_build(topic: &str) -> (VectorSink, crate::sinks::Healthcheck) {
        let cx = SinkContext::new_test();
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
        components::run_sink(sink, events, &HTTP_SINK_TAGS).await;
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
            let expected = serde_json::to_value(input[i].as_log().all_fields()).unwrap();
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
        sink.run(events).await.expect("Sending events failed");
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
        let url = format!("{}/v1/projects/{}/{}", emulator_address(), PROJECT, path);
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
            let data = base64::decode(&self.data).expect("Invalid base64 data");
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
