use std::num::NonZeroUsize;

use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::owned_value_path,
};
use vrl::value::Kind;

use crate::{
    aws::{auth::AwsAuthentication, create_client, region::RegionOrEndpoint},
    codecs::DecodingConfig,
    common::sqs::SqsClientBuilder,
    config::{SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput},
    serde::{default_decoding, default_framing_message_based},
    sources::aws_sqs::source::SqsSource,
    tls::TlsConfig,
};

/// Configuration for the `aws_sqs` source.
#[configurable_component(source("aws_sqs", "Collect logs from AWS SQS."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct AwsSqsConfig {
    #[serde(flatten)]
    pub region: RegionOrEndpoint,

    #[serde(default)]
    pub auth: AwsAuthentication,

    /// The URL of the SQS queue to poll for messages.
    #[configurable(metadata(
        docs::examples = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
    ))]
    pub queue_url: String,

    /// How long to wait while polling the queue for new messages, in seconds.
    ///
    /// Generally, this should not be changed unless instructed to do so, as if messages are available,
    /// they are always consumed, regardless of the value of `poll_secs`.
    // NOTE: We restrict this to u32 for safe conversion to i32 later.
    // NOTE: This value isn't used as a `Duration` downstream, so we don't bother using `serde_with`
    #[serde(default = "default_poll_secs")]
    #[derivative(Default(value = "default_poll_secs()"))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Poll Wait Time"))]
    pub poll_secs: u32,

    /// The visibility timeout to use for messages, in seconds.
    ///
    /// This controls how long a message is left unavailable after it is received. If a message is received, and
    /// takes longer than `visibility_timeout_secs` to process and delete the message from the queue, it is made available again for another consumer.
    ///
    /// This can happen if there is an issue between consuming a message and deleting it.
    // NOTE: We restrict this to u32 for safe conversion to i32 later.
    // NOTE: This value isn't used as a `Duration` downstream, so we don't bother using `serde_with`
    #[serde(default = "default_visibility_timeout_secs")]
    #[derivative(Default(value = "default_visibility_timeout_secs()"))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Visibility Timeout"))]
    pub(super) visibility_timeout_secs: u32,

    /// Whether to delete the message once it is processed.
    ///
    /// It can be useful to set this to `false` for debugging or during the initial setup.
    #[serde(default = "default_true")]
    #[derivative(Default(value = "default_true()"))]
    pub(super) delete_message: bool,

    /// Maximum number of concurrent tasks to create for polling the queue for messages.
    ///
    /// Defaults to the number of available CPUs on the system.
    ///
    /// Vector starts with one polling task and dynamically scales up to `max_concurrency` tasks
    /// as message volume increases, scaling back down when the queue is idle.
    pub max_concurrency: Option<NonZeroUsize>,

    /// Number of concurrent tasks to create for polling the queue for messages.
    ///
    /// Defaults to the number of available CPUs on the system.
    ///
    /// Should not typically need to be changed, but it can sometimes be beneficial to raise this
    /// value when there is a high rate of messages being pushed into the queue and the messages
    /// being fetched are small. In these cases, system resources may not be fully utilized without
    /// fetching more messages per second, as it spends more time fetching the messages than
    /// processing them.
    #[configurable(deprecated = "This option has been deprecated, use `max_concurrency` instead.")]
    pub client_concurrency: Option<NonZeroUsize>,

    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,

    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    #[serde(default, deserialize_with = "crate::serde::bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,

    pub tls: Option<TlsConfig>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_sqs")]
impl SourceConfig for AwsSqsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let client = self.build_client(&cx).await?;
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        Ok(crate::sources::opaque_source(
            SqsSource {
                client,
                queue_url: self.queue_url.clone(),
                decoder,
                poll_secs: self.poll_secs,
                max_concurrency: self.resolve_max_concurrency(),
                visibility_timeout_secs: self.visibility_timeout_secs,
                delete_message: self.delete_message,
                acknowledgements,
                log_namespace,
            }
            .run(cx.out, cx.shutdown),
        ))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let schema_definition = self
            .decoding
            .schema_definition(global_log_namespace.merge(self.log_namespace))
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("timestamp"))),
                &owned_value_path!("timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            );

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl AwsSqsConfig {
    pub(super) fn resolve_max_concurrency(&self) -> usize {
        match (self.max_concurrency, self.client_concurrency) {
            (Some(max), Some(_)) => {
                warn!(
                    "The `client_concurrency` setting is deprecated and ignored because `max_concurrency` is also set."
                );
                max.get()
            }
            (Some(max), None) => max.get(),
            (None, Some(legacy)) => {
                warn!(
                    "The `client_concurrency` setting is deprecated, use `max_concurrency` instead."
                );
                legacy.get()
            }
            (None, None) => crate::num_threads(),
        }
    }

    async fn build_client(&self, cx: &SourceContext) -> crate::Result<aws_sdk_sqs::Client> {
        create_client::<SqsClientBuilder>(
            &SqsClientBuilder {},
            &self.auth,
            self.region.region(),
            self.region.endpoint(),
            &cx.proxy,
            self.tls.as_ref(),
            None,
        )
        .await
    }
}

const fn default_poll_secs() -> u32 {
    15
}

const fn default_visibility_timeout_secs() -> u32 {
    300
}

const fn default_true() -> bool {
    true
}

impl_generate_config_from_default!(AwsSqsConfig);

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::AwsSqsConfig;

    #[test]
    fn resolve_max_concurrency_defaults_to_num_threads() {
        let config = AwsSqsConfig::default();
        assert_eq!(config.resolve_max_concurrency(), crate::num_threads());
    }

    #[test]
    fn resolve_max_concurrency_uses_max_concurrency() {
        let config = AwsSqsConfig {
            max_concurrency: Some(NonZeroUsize::new(12).unwrap()),
            ..Default::default()
        };
        assert_eq!(config.resolve_max_concurrency(), 12);
    }

    #[test]
    fn resolve_max_concurrency_falls_back_to_deprecated_client_concurrency() {
        let config = AwsSqsConfig {
            client_concurrency: Some(NonZeroUsize::new(6).unwrap()),
            ..Default::default()
        };
        assert_eq!(config.resolve_max_concurrency(), 6);
    }

    #[test]
    fn resolve_max_concurrency_prefers_max_concurrency_over_client_concurrency() {
        let config = AwsSqsConfig {
            max_concurrency: Some(NonZeroUsize::new(16).unwrap()),
            client_concurrency: Some(NonZeroUsize::new(4).unwrap()),
            ..Default::default()
        };
        assert_eq!(config.resolve_max_concurrency(), 16);
    }

    #[test]
    fn deserialize_yaml_rejects_zero_max_concurrency() {
        let yaml = r#"
            queue_url: "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
            max_concurrency: 0
        "#;
        assert!(serde_yaml::from_str::<AwsSqsConfig>(yaml).is_err());
    }
}
