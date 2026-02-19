use aws_sdk_sqs::Client as SqsClient;
use vector_lib::configurable::configurable_component;

use super::{
    BaseSSSinkConfig, SSRequestBuilder, SSSink,
    client::{SqsBatchMessagePublisher, SqsMessagePublisher},
    message_deduplication_id, message_group_id,
};
use crate::{
    aws::{RegionOrEndpoint, create_client},
    common::sqs::SqsClientBuilder,
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig, SinkConfig,
        SinkContext,
    },
    sinks::util::{BatchConfig, SinkBatchSettings},
};

/// Default batch settings for the SQS sink.
/// Uses 256KB as the safe default for max_bytes (standard SQS queue limit).
/// Users with extended message size queues (1MB) can explicitly set max_bytes = 1048576.
#[derive(Clone, Copy, Debug, Default)]
pub struct SqsDefaultBatchSettings;

impl SinkBatchSettings for SqsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(262_144); // 256KB safe default
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `aws_sqs` sink.
#[configurable_component(sink(
    "aws_sqs",
    "Publish observability events to AWS Simple Queue Service topics."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct SqsSinkConfig {
    /// The URL of the Amazon SQS queue to which messages are sent.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(
        docs::examples = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
    ))]
    pub(super) queue_url: String,

    #[serde(flatten)]
    pub(super) region: RegionOrEndpoint,

    /// Event batching behavior.
    ///
    /// When configured, multiple events will be sent in a single request using the
    /// `send_message_batch` API, reducing the number of API calls by up to 10x.
    ///
    /// ## Retry Behavior
    ///
    /// Uses **all-or-nothing** semantics: if any message in a batch fails to send, the **entire batch is retried**
    /// by Vector's retry framework. This approach simplifies error handling and leverages Vector's built-in
    /// deduplication and acknowledgements to prevent message loss.
    ///
    /// Per-message retry is not used because:
    /// - SQS batch limit is only 10 messages—low cost to retry all
    /// - Simpler than maintaining per-message state
    /// - Aligns with Vector's request-level deduplication semantics
    ///
    /// SQS limits batches to a maximum of 10 messages or 256KB (standard queues), upgradable to 1MB.
    /// The default batch size is set to 256KB to ensure compatibility with standard queues, but can be increased for extended queues.
    ///
    /// Note: Batching introduces latency based on the `timeout_secs` setting.
    /// If omitted, messages are sent individually (legacy behavior).
    #[configurable(derived)]
    #[serde(default)]
    pub(super) batch: BatchConfig<SqsDefaultBatchSettings>,

    #[serde(flatten)]
    pub(super) base_config: BaseSSSinkConfig,
}

impl GenerateConfig for SqsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"queue_url = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
            region = "us-east-2"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

impl SqsSinkConfig {
    pub(super) async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<SqsClient> {
        create_client::<SqsClientBuilder>(
            &SqsClientBuilder {},
            &self.base_config.auth,
            self.region.region(),
            self.region.endpoint(),
            proxy,
            self.base_config.tls.as_ref(),
            None,
        )
        .await
    }

    /// Determines if batching is enabled by checking if any batch settings are configured.
    #[allow(dead_code)] // Used in build() for routing
    fn batching_enabled(&self) -> bool {
        self.batch.max_events.is_some() || self.batch.timeout_secs.is_some()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_sqs")]
impl SinkConfig for SqsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(crate::sinks::VectorSink, crate::sinks::Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        let healthcheck = Box::pin(healthcheck(client.clone(), self.queue_url.clone()));

        let message_group_id = message_group_id(
            self.base_config.message_group_id.clone(),
            self.queue_url.ends_with(".fifo"),
        )?;
        let message_deduplication_id =
            message_deduplication_id(self.base_config.message_deduplication_id.clone())?;

        if self.batching_enabled() {
            // New batched path using send_message_batch API
            let batch_settings = self
                .batch
                .validate()?
                .limit_max_events(10)? // SQS API limit
                .limit_max_bytes(1_048_576)? // Max with extended client library
                .into_batcher_settings()?;

            let publisher = SqsBatchMessagePublisher::new(client.clone(), self.queue_url.clone());
            let request_builder = SSRequestBuilder::new(
                message_group_id,
                message_deduplication_id,
                self.base_config.encoding.clone(),
            )?;

            let sink = super::batch_sink::BatchedSqsSink::new(
                batch_settings,
                request_builder,
                self.base_config.request,
                publisher,
            )?;

            Ok((
                crate::sinks::VectorSink::from_event_streamsink(sink),
                healthcheck,
            ))
        } else {
            // Legacy non-batched path using send_message API
            let publisher = SqsMessagePublisher::new(client.clone(), self.queue_url.clone());
            let sink = SSSink::new(
                SSRequestBuilder::new(
                    message_group_id,
                    message_deduplication_id,
                    self.base_config.encoding.clone(),
                )?,
                self.base_config.request,
                publisher,
            )?;
            Ok((
                crate::sinks::VectorSink::from_event_streamsink(sink),
                healthcheck,
            ))
        }
    }

    fn input(&self) -> Input {
        Input::new(self.base_config.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.base_config.acknowledgements
    }
}

pub(super) async fn healthcheck(client: SqsClient, queue_url: String) -> crate::Result<()> {
    client
        .get_queue_attributes()
        .queue_url(queue_url)
        .send()
        .await
        .map(|_| ())
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aws::RegionOrEndpoint;
    use vector_lib::codecs::JsonSerializerConfig;

    const TEST_REGION: &str = "us-east-2";

    fn create_test_base_config() -> BaseSSSinkConfig {
        BaseSSSinkConfig {
            encoding: JsonSerializerConfig::default().into(),
            message_group_id: None,
            message_deduplication_id: None,
            request: Default::default(),
            tls: None,
            assume_role: None,
            auth: Default::default(),
            acknowledgements: Default::default(),
        }
    }

    #[test]
    fn batching_disabled_by_default() {
        let config = SqsSinkConfig {
            queue_url: "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue".to_string(),
            region: RegionOrEndpoint::with_region(String::from(TEST_REGION)),
            batch: Default::default(),
            base_config: create_test_base_config(),
        };

        assert!(
            !config.batching_enabled(),
            "Batching should be disabled by default"
        );
    }

    #[test]
    fn batching_enabled_with_max_events() {
        let mut batch = BatchConfig::default();
        batch.max_events = Some(10);

        let config = SqsSinkConfig {
            queue_url: "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue".to_string(),
            region: RegionOrEndpoint::with_region(String::from(TEST_REGION)),
            batch,
            base_config: create_test_base_config(),
        };

        assert!(
            config.batching_enabled(),
            "Batching should be enabled when max_events is set"
        );
    }

    #[test]
    fn batching_enabled_with_timeout() {
        let mut batch = BatchConfig::default();
        batch.timeout_secs = Some(0.5);

        let config = SqsSinkConfig {
            queue_url: "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue".to_string(),
            region: RegionOrEndpoint::with_region(String::from(TEST_REGION)),
            batch,
            base_config: create_test_base_config(),
        };

        assert!(
            config.batching_enabled(),
            "Batching should be enabled when timeout_secs is set"
        );
    }

    #[test]
    fn batching_enabled_with_max_bytes() {
        let mut batch = BatchConfig::default();
        batch.max_bytes = Some(1_048_576); // 1MB

        let config = SqsSinkConfig {
            queue_url: "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue".to_string(),
            region: RegionOrEndpoint::with_region(String::from(TEST_REGION)),
            batch,
            base_config: create_test_base_config(),
        };

        // Note: max_bytes alone doesn't enable batching per our logic
        // User must set max_events or timeout_secs
        assert!(
            !config.batching_enabled(),
            "Batching requires max_events or timeout_secs"
        );
    }

    #[test]
    fn batch_settings_default_to_256kb() {
        // Verify our default batch settings
        assert_eq!(
            SqsDefaultBatchSettings::MAX_BYTES,
            Some(262_144),
            "Default max_bytes should be 256KB"
        );
        assert_eq!(
            SqsDefaultBatchSettings::MAX_EVENTS,
            None,
            "Default max_events should be None"
        );
        assert_eq!(
            SqsDefaultBatchSettings::TIMEOUT_SECS,
            1.0,
            "Default timeout should be 1 second"
        );
    }

    #[test]
    fn batch_validation_enforces_sqs_limits() {
        let mut batch = BatchConfig::default();
        batch.max_events = Some(15); // Exceeds SQS limit of 10
        batch.timeout_secs = Some(1.0);

        let config = SqsSinkConfig {
            queue_url: "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue".to_string(),
            region: RegionOrEndpoint::with_region(String::from(TEST_REGION)),
            batch,
            base_config: create_test_base_config(),
        };

        // The limit is enforced during validation
        let result = config.batch.validate().and_then(|b| b.limit_max_events(10));

        assert!(result.is_err(), "Should reject max_events > 10");
    }

    #[test]
    fn batch_validation_allows_1mb_explicit() {
        let mut batch = BatchConfig::default();
        batch.max_events = Some(10);
        batch.max_bytes = Some(1_048_576); // 1MB explicit
        batch.timeout_secs = Some(1.0);

        let config = SqsSinkConfig {
            queue_url: "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue".to_string(),
            region: RegionOrEndpoint::with_region(String::from(TEST_REGION)),
            batch,
            base_config: create_test_base_config(),
        };

        // Should accept 1MB when explicitly set
        let result = config
            .batch
            .validate()
            .and_then(|b| b.limit_max_bytes(1_048_576));

        assert!(result.is_ok(), "Should allow 1MB when explicitly set");
    }
}
