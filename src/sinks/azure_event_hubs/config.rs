use vector_lib::lookup::lookup_v2::OptionalTargetPath;

use crate::{sinks::prelude::*, sources::azure_event_hubs::build_credential};

use super::sink::AzureEventHubsSink;

/// Configuration for the `azure_event_hubs` sink.
#[configurable_component(sink("azure_event_hubs", "Send events to Azure Event Hubs."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AzureEventHubsSinkConfig {
    /// The connection string for the Event Hubs namespace.
    ///
    /// If not set, authentication falls back to `azure_identity` (e.g., Managed Identity).
    /// In that case, `namespace` and `event_hub_name` must be provided.
    #[configurable(metadata(
        docs::examples = "Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=mykeyname;SharedAccessKey=mykey;EntityPath=my-event-hub"
    ))]
    pub connection_string: Option<vector_lib::sensitive_string::SensitiveString>,

    /// The fully qualified Event Hubs namespace host.
    ///
    /// Required when not using a connection string.
    #[configurable(metadata(docs::examples = "mynamespace.servicebus.windows.net"))]
    pub namespace: Option<String>,

    /// The name of the Event Hub to send events to.
    #[configurable(metadata(docs::examples = "my-event-hub"))]
    pub event_hub_name: Option<String>,

    /// The log field to use as the Event Hubs partition ID.
    ///
    /// If set, events are routed to the specified partition. If not set,
    /// Event Hubs automatically selects a partition (round-robin).
    pub partition_id_field: Option<OptionalTargetPath>,

    /// Whether to batch events before sending.
    ///
    /// When enabled, events are accumulated per partition and sent as an `EventDataBatch`,
    /// preserving per-partition ordering. When disabled, each event is sent individually.
    #[serde(default = "default_batch_enabled")]
    pub batch_enabled: bool,

    /// Maximum number of events to accumulate before flushing a batch.
    ///
    /// Only used when `batch_enabled` is `true`.
    #[serde(default = "default_batch_max_events")]
    #[configurable(metadata(docs::examples = 100))]
    pub batch_max_events: usize,

    /// Maximum time to wait before flushing a batch, in seconds.
    ///
    /// Only used when `batch_enabled` is `true`.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(default = "default_batch_timeout_secs")]
    pub batch_timeout_secs: u64,

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

    /// Maximum number of retry attempts for failed sends.
    ///
    /// The SDK uses exponential backoff between retries.
    #[serde(default = "default_retry_max_retries")]
    #[configurable(metadata(docs::examples = 8))]
    pub retry_max_retries: u32,

    /// Initial delay before the first retry, in milliseconds.
    #[configurable(metadata(docs::type_unit = "milliseconds"))]
    #[serde(default = "default_retry_initial_delay_ms")]
    pub retry_initial_delay_ms: u64,

    /// Maximum total time for all retry attempts, in seconds.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(default = "default_retry_max_elapsed_secs")]
    pub retry_max_elapsed_secs: u64,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

const fn default_batch_enabled() -> bool {
    true
}

const fn default_batch_max_events() -> usize {
    100
}

const fn default_batch_timeout_secs() -> u64 {
    1
}

const fn default_rate_limit_duration_secs() -> u64 {
    1
}

const fn default_rate_limit_num() -> u64 {
    i64::MAX as u64
}

const fn default_retry_max_retries() -> u32 {
    8
}

const fn default_retry_initial_delay_ms() -> u64 {
    200
}

const fn default_retry_max_elapsed_secs() -> u64 {
    60
}

impl GenerateConfig for AzureEventHubsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"connection_string = "Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=mykeyname;SharedAccessKey=mykey;EntityPath=my-event-hub"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "azure_event_hubs")]
impl SinkConfig for AzureEventHubsSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = AzureEventHubsSink::new(self).await?;

        let connection_string = self.connection_string.clone();
        let namespace = self.namespace.clone();
        let event_hub_name = self.event_hub_name.clone();
        let healthcheck = async move {
            run_healthcheck(
                connection_string.as_ref(),
                namespace.as_deref(),
                event_hub_name.as_deref(),
            )
            .await
        }
        .boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

async fn run_healthcheck(
    connection_string: Option<&vector_lib::sensitive_string::SensitiveString>,
    namespace: Option<&str>,
    event_hub_name: Option<&str>,
) -> crate::Result<()> {
    use azure_messaging_eventhubs::ProducerClient;

    let (ns, eh_name, credential, custom_endpoint) =
        build_credential(connection_string, namespace, event_hub_name)?;

    let mut builder = ProducerClient::builder();
    if let Some(endpoint) = custom_endpoint {
        builder = builder.with_custom_endpoint(endpoint);
    }
    let client = builder
        .open(&ns, &eh_name, credential)
        .await
        .map_err(|e| format!("Event Hubs healthcheck: failed to create producer: {e}"))?;

    client
        .get_eventhub_properties()
        .await
        .map_err(|e| format!("Event Hubs healthcheck failed: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AzureEventHubsSinkConfig>();
    }

    #[test]
    fn config_from_toml_connection_string() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        assert!(config.connection_string.is_some());
        assert!(config.namespace.is_none());
        assert!(config.event_hub_name.is_none());
    }

    #[test]
    fn config_from_toml_identity_auth() {
        let toml_str = r#"
            namespace = "myns.servicebus.windows.net"
            event_hub_name = "my-hub"
            encoding.codec = "text"
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        assert!(config.connection_string.is_none());
        assert_eq!(
            config.namespace.as_deref(),
            Some("myns.servicebus.windows.net")
        );
        assert_eq!(config.event_hub_name.as_deref(), Some("my-hub"));
    }

    #[test]
    fn config_from_toml_with_rate_limit() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
            rate_limit_duration_secs = 2
            rate_limit_num = 500
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.rate_limit_duration_secs, 2);
        assert_eq!(config.rate_limit_num, 500);
    }

    #[test]
    fn config_defaults_acknowledgements() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        // Default acknowledgements should not be enabled
        assert!(!config.acknowledgements.enabled());
    }

    #[test]
    fn config_defaults_batch() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        assert!(config.batch_enabled);
        assert_eq!(config.batch_max_events, 100);
        assert_eq!(config.batch_timeout_secs, 1);
    }

    #[test]
    fn config_defaults_retry() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.retry_max_retries, 8);
        assert_eq!(config.retry_initial_delay_ms, 200);
        assert_eq!(config.retry_max_elapsed_secs, 60);
    }

    #[test]
    fn config_custom_batch_settings() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
            batch_enabled = false
            batch_max_events = 50
            batch_timeout_secs = 5
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.batch_enabled);
        assert_eq!(config.batch_max_events, 50);
        assert_eq!(config.batch_timeout_secs, 5);
    }

    #[test]
    fn config_custom_retry_settings() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
            retry_max_retries = 3
            retry_initial_delay_ms = 500
            retry_max_elapsed_secs = 30
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.retry_max_retries, 3);
        assert_eq!(config.retry_initial_delay_ms, 500);
        assert_eq!(config.retry_max_elapsed_secs, 30);
    }

    #[test]
    fn config_partition_id_field() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
            partition_id_field = ".partition"
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        assert!(config.partition_id_field.is_some());
    }

    #[test]
    fn config_all_fields() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
            partition_id_field = ".pid"
            batch_enabled = true
            batch_max_events = 200
            batch_timeout_secs = 3
            rate_limit_duration_secs = 5
            rate_limit_num = 100
            retry_max_retries = 4
            retry_initial_delay_ms = 300
            retry_max_elapsed_secs = 120
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        assert!(config.partition_id_field.is_some());
        assert!(config.batch_enabled);
        assert_eq!(config.batch_max_events, 200);
        assert_eq!(config.batch_timeout_secs, 3);
        assert_eq!(config.rate_limit_duration_secs, 5);
        assert_eq!(config.rate_limit_num, 100);
        assert_eq!(config.retry_max_retries, 4);
        assert_eq!(config.retry_initial_delay_ms, 300);
        assert_eq!(config.retry_max_elapsed_secs, 120);
    }

    #[test]
    fn config_rejects_unknown_fields() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
            unknown_field = "should_fail"
        "#;
        let result = toml::from_str::<AzureEventHubsSinkConfig>(toml_str);
        assert!(result.is_err());
    }
}
