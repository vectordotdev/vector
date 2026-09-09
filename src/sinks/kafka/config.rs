use std::{collections::HashMap, time::Duration};

use futures::FutureExt;
use rdkafka::ClientConfig;
use serde_with::serde_as;
use vector_lib::{
    codecs::JsonSerializerConfig, configurable::configurable_component,
    lookup::lookup_v2::ConfigTargetPath,
};
use vrl::value::Kind;

use crate::{
    config::ValidatedSink,
    kafka::{KafkaAuthConfig, KafkaCompression},
    serde::json::to_string,
    sinks::{
        kafka::sink::{KafkaSink, healthcheck},
        prelude::*,
    },
    template::ConfinementConfig,
};

/// Configuration for the `kafka` sink.
#[serde_as]
#[configurable_component(sink(
    "kafka",
    "Publish observability event data to Apache Kafka topics."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KafkaSinkConfig {
    /// A comma-separated list of Kafka bootstrap servers.
    ///
    /// These are the servers in a Kafka cluster that a client should use to bootstrap its
    /// connection to the cluster, allowing discovery of all the other hosts in the cluster.
    ///
    /// Must be in the form of `host:port`, and comma-separated.
    #[configurable(metadata(docs::examples = "10.14.22.123:9092,10.14.23.332:9092"))]
    pub bootstrap_servers: String,

    /// The Kafka topic name to write events to.
    #[configurable(metadata(docs::templateable))]
    #[configurable(metadata(
        docs::examples = "topic-1234",
        docs::examples = "logs-{{unit}}-%Y-%m-%d"
    ))]
    pub topic: Template,

    /// The topic name to use for healthcheck. If omitted, `topic` is used.
    /// This option helps prevent healthcheck warnings when `topic` is templated.
    ///
    /// It is ignored when healthcheck is disabled.
    pub healthcheck_topic: Option<String>,

    /// The log field name or tag key to use for the topic key.
    ///
    /// If the field does not exist in the log or in the tags, a blank value is used. If
    /// unspecified, the key is not sent.
    ///
    /// Kafka uses a hash of the key to choose the partition or uses round-robin if the record has
    /// no key.
    #[configurable(metadata(docs::examples = "user_id"))]
    #[configurable(metadata(docs::examples = ".my_topic"))]
    #[configurable(metadata(docs::examples = "%my_topic"))]
    pub key_field: Option<ConfigTargetPath>,

    pub encoding: EncodingConfig,

    // These batching options will **not** override librdkafka_options values.
    #[serde(default)]
    pub batch: BatchConfig<NoDefaultsBatchSettings>,

    #[serde(default)]
    pub compression: KafkaCompression,

    #[serde(flatten)]
    pub auth: KafkaAuthConfig,

    /// Default timeout, in milliseconds, for network requests.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[serde(default = "default_socket_timeout_ms")]
    #[configurable(metadata(docs::examples = 30000, docs::examples = 60000))]
    #[configurable(metadata(docs::human_name = "Socket Timeout"))]
    pub socket_timeout_ms: Duration,

    /// Local message timeout, in milliseconds.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 150000, docs::examples = 450000))]
    #[serde(default = "default_message_timeout_ms")]
    #[configurable(metadata(docs::human_name = "Message Timeout"))]
    pub message_timeout_ms: Duration,

    /// The time window used for the `rate_limit_num` option.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Rate Limit Duration"))]
    #[serde(default = "default_rate_limit_duration_secs")]
    pub rate_limit_duration_secs: u64,

    /// The maximum number of requests allowed within the `rate_limit_duration_secs` time window.
    #[configurable(metadata(docs::type_unit = "requests"))]
    #[configurable(metadata(docs::human_name = "Rate Limit Number"))]
    #[serde(default = "default_rate_limit_num")]
    pub rate_limit_num: u64,

    /// A map of advanced options to pass directly to the underlying `librdkafka` client.
    ///
    /// For more information on configuration options, see [Configuration properties][config_props_docs].
    ///
    /// [config_props_docs]: https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md
    #[serde(default)]
    #[configurable(metadata(docs::examples = "example_librdkafka_options()"))]
    #[configurable(metadata(
        docs::additional_props_description = "A librdkafka configuration option."
    ))]
    pub librdkafka_options: HashMap<String, String>,

    /// The log field name to use for the Kafka headers.
    ///
    /// If omitted, no headers are written.
    #[serde(alias = "headers_field")] // accidentally released as `headers_field` in 0.18
    #[configurable(metadata(docs::examples = "headers"))]
    pub headers_key: Option<ConfigTargetPath>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

const fn default_socket_timeout_ms() -> Duration {
    Duration::from_millis(60000) // default in librdkafka
}

const fn default_message_timeout_ms() -> Duration {
    Duration::from_millis(300000) // default in librdkafka
}

const fn default_rate_limit_duration_secs() -> u64 {
    1
}

const fn default_rate_limit_num() -> u64 {
    i64::MAX as u64 // i64 avoids TOML deserialize issue
}

fn example_librdkafka_options() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter([
        ("client.id".to_string(), "${ENV_VAR}".to_string()),
        ("fetch.error.backoff.ms".to_string(), "1000".to_string()),
        ("socket.send.buffer.bytes".to_string(), "100".to_string()),
    ])
}

impl KafkaSinkConfig {
    pub(crate) fn to_rdkafka(&self) -> crate::Result<ClientConfig> {
        self.validate_batch_librdkafka_conflicts()?;

        let mut client_config = ClientConfig::new();
        client_config
            .set("bootstrap.servers", &self.bootstrap_servers)
            .set(
                "socket.timeout.ms",
                self.socket_timeout_ms.as_millis().to_string(),
            )
            .set("statistics.interval.ms", "1000");

        self.auth.apply(&mut client_config)?;

        // All batch options are producer only.
        client_config
            .set("compression.codec", to_string(self.compression))
            .set(
                "message.timeout.ms",
                self.message_timeout_ms.as_millis().to_string(),
            );

        if let Some(value) = self.batch.timeout_secs {
            // Delay in milliseconds to wait for messages in the producer queue to accumulate before
            // constructing message batches (MessageSets) to transmit to brokers. A higher value
            // allows larger and more effective (less overhead, improved compression) batches of
            // messages to accumulate at the expense of increased message delivery latency.
            // Type: float
            let key = "queue.buffering.max.ms";
            debug!(
                librdkafka_option = key,
                batch_option = "timeout_secs",
                value,
                "Applying batch option as librdkafka option."
            );
            client_config.set(key, (value * 1000.0).round().to_string());
        }
        if let Some(value) = self.batch.max_events {
            // Maximum number of messages batched in one MessageSet. The total MessageSet size is
            // also limited by batch.size and message.max.bytes.
            // Type: integer
            let key = "batch.num.messages";
            debug!(
                librdkafka_option = key,
                batch_option = "max_events",
                value,
                "Applying batch option as librdkafka option."
            );
            client_config.set(key, value.to_string());
        }
        if let Some(value) = self.batch.max_bytes {
            // Maximum size (in bytes) of all messages batched in one MessageSet, including protocol
            // framing overhead. This limit is applied after the first message has been added to the
            // batch, regardless of the first message's size, this is to ensure that messages that
            // exceed batch.size are produced. The total MessageSet size is also limited by
            // batch.num.messages and message.max.bytes.
            // Type: integer
            let key = "batch.size";
            debug!(
                librdkafka_option = key,
                batch_option = "max_bytes",
                value,
                "Applying batch option as librdkafka option."
            );
            client_config.set(key, value.to_string());
        }

        for (key, value) in self.librdkafka_options.iter() {
            debug!(option = %key, value = %value, "Setting librdkafka option.");
            client_config.set(key.as_str(), value.as_str());
        }

        Ok(client_config)
    }

    /// Validate that no Vector batch option conflicts with a corresponding
    /// `librdkafka_options` key.
    ///
    /// `to_rdkafka` maps each batch option to a specific librdkafka option and
    /// refuses to set both. This is a pure configuration error, so it is checked
    /// here (and reused by `to_rdkafka`) without building a producer.
    fn validate_batch_librdkafka_conflicts(&self) -> crate::Result<()> {
        if let Some(value) = self.batch.timeout_secs {
            Self::ensure_no_librdkafka_conflict(
                "batch.timeout_secs",
                "queue.buffering.max.ms",
                value,
                &self.librdkafka_options,
            )?;
        }
        if let Some(value) = self.batch.max_events {
            Self::ensure_no_librdkafka_conflict(
                "batch.max_events",
                "batch.num.messages",
                value,
                &self.librdkafka_options,
            )?;
        }
        if let Some(value) = self.batch.max_bytes {
            Self::ensure_no_librdkafka_conflict(
                "batch.max_bytes",
                "batch.size",
                value,
                &self.librdkafka_options,
            )?;
        }
        Ok(())
    }

    /// Reject a Vector batch option that would overwrite a corresponding
    /// `librdkafka_options` key.
    fn ensure_no_librdkafka_conflict(
        batch_option: &str,
        key: &str,
        value: impl std::fmt::Display,
        librdkafka_options: &HashMap<String, String>,
    ) -> crate::Result<()> {
        if let Some(val) = librdkafka_options.get(key) {
            return Err(format!(
                "Batching setting `{batch_option}` sets `librdkafka_options.{key}={value}`.\
                The config already sets this as `librdkafka_options.{key}={val}`.\
                Please delete one."
            )
            .into());
        }
        Ok(())
    }
}

impl GenerateConfig for KafkaSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            bootstrap_servers: "10.14.22.123:9092,10.14.23.332:9092".to_owned(),
            topic: Template::try_from("topic-1234".to_owned()).unwrap(),
            healthcheck_topic: None,
            key_field: Some(ConfigTargetPath::try_from("user_id".to_owned()).unwrap()),
            encoding: JsonSerializerConfig::default().into(),
            batch: Default::default(),
            compression: KafkaCompression::None,
            auth: Default::default(),
            socket_timeout_ms: default_socket_timeout_ms(),
            message_timeout_ms: default_message_timeout_ms(),
            rate_limit_duration_secs: default_rate_limit_duration_secs(),
            rate_limit_num: default_rate_limit_num(),
            librdkafka_options: Default::default(),
            headers_key: None,
            acknowledgements: Default::default(),
            confinement: ConfinementConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kafka")]
impl SinkConfig for KafkaSinkConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        let requirements = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::new(self.encoding.config().input_type()).with_schema_requirement(requirements)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedKafkaSink {
    topic: ConfinedTemplate,
}

#[async_trait::async_trait]
impl ValidatedSink for KafkaSinkConfig {
    type Validated = ValidatedKafkaSink;
    fn validate(&self) -> crate::Result<ValidatedKafkaSink> {
        // Build the librdkafka ClientConfig (pure: just key-value pairs) to
        // surface batch/librdkafka conflicts. Native config creation — which
        // can load `plugin.library.paths` and run plugin initialization — is
        // deferred to `build()` per the split-component-build-lifecycle RFC.
        let _ = self.to_rdkafka()?;
        let topic = self
            .topic
            .clone()
            .confine(&self.confinement, Self::NAME, "topic")?;
        Ok(ValidatedKafkaSink { topic })
    }

    async fn build(
        &self,
        validated: &ValidatedKafkaSink,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedKafkaSink { topic } = validated;
        let sink = KafkaSink::new(self.clone(), topic.clone())?;
        let hc = healthcheck(self.clone(), topic.clone(), cx.healthcheck.clone()).boxed();
        Ok((VectorSink::from_event_streamsink(sink), hc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ValidatedSink;
    use crate::template::{ConfinementConfig, Template};

    #[test]
    fn generate_config() {
        KafkaSinkConfig::generate_config();
    }

    #[test]
    fn validate_returns_confined_topic() {
        let config: KafkaSinkConfig = serde_yaml::from_str(
            r#"
            bootstrap_servers: "localhost:9092"
            topic: "test-topic"
            encoding:
                codec: "json"
            "#,
        )
        .unwrap();
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(validated.topic.to_string(), "test-topic");
    }

    #[test]
    fn confinement_rejects_unconfined_topic() {
        let template = Template::try_from("{{ topic }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "kafka", "topic");
        assert!(result.is_err());
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_topic() {
        let template = Template::try_from("{{ topic }}").unwrap();
        let config = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let result = template.confine(&config, "kafka", "topic");
        assert!(result.is_ok());
    }

    #[test]
    fn confinement_allows_prefixed_topic() {
        let template = Template::try_from("events-{{ env }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "kafka", "topic");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_batch_timeout_secs_conflicting_with_librdkafka_option() {
        let config: KafkaSinkConfig = serde_yaml::from_str(
            r#"
            bootstrap_servers: "localhost:9092"
            topic: "test-topic"
            encoding:
                codec: "json"
            batch:
                timeout_secs: 1.0
            librdkafka_options:
                queue.buffering.max.ms: "1000"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_err(),
            "batch.timeout_secs conflicting with librdkafka_options.queue.buffering.max.ms should fail validation"
        );
    }

    #[test]
    fn validate_rejects_batch_max_events_conflicting_with_librdkafka_option() {
        let config: KafkaSinkConfig = serde_yaml::from_str(
            r#"
            bootstrap_servers: "localhost:9092"
            topic: "test-topic"
            encoding:
                codec: "json"
            batch:
                max_events: 1000
            librdkafka_options:
                batch.num.messages: "1000"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_err(),
            "batch.max_events conflicting with librdkafka_options.batch.num.messages should fail validation"
        );
    }

    #[test]
    fn validate_rejects_batch_max_bytes_conflicting_with_librdkafka_option() {
        let config: KafkaSinkConfig = serde_yaml::from_str(
            r#"
            bootstrap_servers: "localhost:9092"
            topic: "test-topic"
            encoding:
                codec: "json"
            batch:
                max_bytes: 1000000
            librdkafka_options:
                batch.size: "1000000"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_err(),
            "batch.max_bytes conflicting with librdkafka_options.batch.size should fail validation"
        );
    }

    #[test]
    fn validate_accepts_batch_options_without_conflicting_librdkafka_options() {
        let config: KafkaSinkConfig = serde_yaml::from_str(
            r#"
            bootstrap_servers: "localhost:9092"
            topic: "test-topic"
            encoding:
                codec: "json"
            batch:
                timeout_secs: 1.0
                max_events: 1000
                max_bytes: 1000000
            librdkafka_options:
                client.id: "vector"
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_ok(),
            "batch options without conflicting librdkafka options should pass validation"
        );
    }

    #[tokio::test]
    async fn build_rejects_unknown_librdkafka_option() {
        let config: KafkaSinkConfig = serde_yaml::from_str(
            r#"
            bootstrap_servers: "localhost:9092"
            topic: "test-topic"
            encoding:
                codec: "json"
            librdkafka_options:
                definitely.not.an.option: "x"
            "#,
        )
        .unwrap();
        let validated = config
            .validate()
            .expect("validation is pure and should succeed");
        assert!(
            ValidatedSink::build(&config, &validated, SinkContext::default())
                .await
                .is_err(),
            "an unknown librdkafka option should fail build"
        );
    }

    #[tokio::test]
    async fn build_rejects_invalid_librdkafka_option_value() {
        let config: KafkaSinkConfig = serde_yaml::from_str(
            r#"
            bootstrap_servers: "localhost:9092"
            topic: "test-topic"
            encoding:
                codec: "json"
            librdkafka_options:
                queue.buffering.max.ms: "not-a-number"
            "#,
        )
        .unwrap();
        let validated = config
            .validate()
            .expect("validation is pure and should succeed");
        assert!(
            ValidatedSink::build(&config, &validated, SinkContext::default())
                .await
                .is_err(),
            "an invalid value for a known librdkafka option should fail build"
        );
    }
}
