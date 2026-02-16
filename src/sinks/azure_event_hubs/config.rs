use vector_lib::lookup::lookup_v2::OptionalTargetPath;

use crate::{
    sinks::prelude::*,
    sources::azure_event_hubs::build_credential,
};

use super::sink::AzureEventHubsSink;

/// Configuration for the `azure_event_hubs` sink.
#[configurable_component(sink(
    "azure_event_hubs",
    "Send events to Azure Event Hubs."
))]
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

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for AzureEventHubsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"connection_string = "Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=mykeyname;SharedAccessKey=mykey;EntityPath=my-event-hub"
            encoding.codec = "json"
            [request]
            concurrency = 10"#,
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
        assert_eq!(config.namespace.as_deref(), Some("myns.servicebus.windows.net"));
        assert_eq!(config.event_hub_name.as_deref(), Some("my-hub"));
    }

    #[test]
    fn config_from_toml_with_request_settings() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            encoding.codec = "json"
            [request]
            concurrency = 20
            timeout_secs = 30
        "#;
        let config: AzureEventHubsSinkConfig = toml::from_str(toml_str).unwrap();
        let settings = config.request.into_settings();
        assert_eq!(settings.concurrency, Some(20));
        assert_eq!(settings.timeout, std::time::Duration::from_secs(30));
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
}
