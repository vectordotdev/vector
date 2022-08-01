use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use codecs::decoding::{DeserializerConfig, FramingConfig};

use crate::{
    config::{AcknowledgementsConfig, Output, SourceConfig, SourceContext},
    kafka::{KafkaAuthConfig, KafkaSaslConfig},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::kafka::{
        default_auto_offset_reset, default_commit_interval_ms, default_fetch_wait_max_ms,
        default_headers_key, default_key_field, default_offset_key, default_partition_key,
        default_session_timeout_ms, default_socket_timeout_ms, default_topic_key,
        KafkaSourceConfig,
    },
    tls::TlsEnableableConfig,
};

/// Configuration for the `azure_event_hubs` source.
/// This component leverages event hubs' compatability with `kafka`.
/// See the documentation [here](event_hubs_docs)
/// for details on how `azure_event_hubs` can use `kafka`.
///
/// [event_hubs_docs]: https://docs.microsoft.com/en-gb/azure/event-hubs/event-hubs-for-kafka-ecosystem-overview
#[configurable_component(source)]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct AzureEventHubsConfig {
    /// The connection string.
    /// See [here](connection_string) for details.
    ///
    /// [connection_string]: https://docs.microsoft.com/en-gb/azure/event-hubs/event-hubs-get-connection-string
    pub connection_string: String,

    /// The namespace name.
    pub namespace: String,

    /// The name of the queue to listen to.
    pub queue_name: String,

    /// The name of the consumer group.
    #[serde(default = "default_group_id")]
    pub group_id: String,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,
}

impl_generate_config_from_default!(AzureEventHubsConfig);

fn default_group_id() -> String {
    "$DEFAULT".to_string()
}

#[async_trait::async_trait]
#[typetag::serde(name = "azure_event_hubs")]
impl SourceConfig for AzureEventHubsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let source = KafkaSourceConfig {
            bootstrap_servers: format!("{}.servicebus.windows.net:9093", self.namespace),
            topics: vec![self.queue_name.clone()],
            group_id: self.group_id.clone(),
            auto_offset_reset: default_auto_offset_reset(),
            session_timeout_ms: default_session_timeout_ms(),
            socket_timeout_ms: default_socket_timeout_ms(),
            fetch_wait_max_ms: default_fetch_wait_max_ms(),
            commit_interval_ms: default_commit_interval_ms(),
            key_field: default_key_field(),
            topic_key: default_topic_key(),
            partition_key: default_partition_key(),
            offset_key: default_offset_key(),
            headers_key: default_headers_key(),
            librdkafka_options: None,
            auth: KafkaAuthConfig {
                sasl: Some(KafkaSaslConfig {
                    enabled: Some(true),
                    username: Some("$ConnectionString".to_string()),
                    password: Some(self.connection_string.clone()),
                    mechanism: Some("PLAIN".to_string()),
                }),
                tls: self.tls.clone(),
            },
            framing: self.framing.clone(),
            decoding: self.decoding.clone(),
            acknowledgements: self.acknowledgements,
        };

        source.build(cx).await
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
    }

    fn source_type(&self) -> &'static str {
        "azure_event_hubs"
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}
