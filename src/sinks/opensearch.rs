use vector_lib::configurable::configurable_component;

use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    sinks::{Healthcheck, VectorSink, elasticsearch::ElasticsearchConfig},
    template::ConfinementConfig,
};

/// Configuration for the `opensearch` sink.
///
/// Uses the Elasticsearch-compatible bulk API supported by OpenSearch. Configuration options
/// and defaults are shared with the `elasticsearch` sink, including automatic API version
/// detection and support for Amazon OpenSearch Service and OpenSearch Serverless.
#[configurable_component(sink("opensearch", "Index observability events in OpenSearch."))]
#[derive(Clone, Debug, Default)]
pub struct OpenSearchConfig(ElasticsearchConfig);

impl GenerateConfig for OpenSearchConfig {
    fn generate_config() -> serde_json::Value {
        ElasticsearchConfig::generate_config()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "opensearch")]
impl SinkConfig for OpenSearchConfig {
    fn confinement_config(&self) -> Option<&ConfinementConfig> {
        self.0.confinement_config()
    }

    fn input(&self) -> Input {
        self.0.input()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        self.0.acknowledgements()
    }
}

#[async_trait::async_trait]
impl ValidatedSink for OpenSearchConfig {
    type Validated = <ElasticsearchConfig as ValidatedSink>::Validated;

    fn validate(&self) -> crate::Result<Self::Validated> {
        self.0.validate()
    }

    async fn build(
        &self,
        validated: &Self::Validated,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        ValidatedSink::build(&self.0, validated, cx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::BoxedSink,
        sinks::elasticsearch::{ElasticsearchApiVersion, OpenSearchServiceType},
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<OpenSearchConfig>();
    }

    #[test]
    fn config_roundtrip() {
        let config: BoxedSink = serde_yaml::from_str(
            r"
            type: opensearch
            endpoints: [http://localhost:9200]
            bulk:
              index: logs-%Y.%m.%d
            acknowledgements:
              enabled: true
            ",
        )
        .unwrap();
        assert!(config.acknowledgements().enabled());
        assert!(config.confinement_config().is_some());
        config.validate_dyn().unwrap();

        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["type"], "opensearch");
        assert_eq!(value["bulk"]["index"], "logs-%Y.%m.%d");
        let roundtrip: BoxedSink = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(roundtrip).unwrap(), value);
    }

    #[test]
    fn defaults_to_managed_with_automatic_api_detection() {
        let config: OpenSearchConfig =
            serde_yaml::from_str("endpoints: [http://localhost:9200]").unwrap();
        assert_eq!(config.0.api_version, ElasticsearchApiVersion::Auto);
        assert_eq!(
            config.0.opensearch_service_type,
            OpenSearchServiceType::Managed
        );
    }

    #[test]
    fn rejects_invalid_config() {
        for yaml in [
            "type: opensearch",
            "type: opensearch\nendpoints: [http://localhost:9200]\nbatch:\n  max_events: 0",
            "type: opensearch\nendpoints: [http://localhost:9200]\nbulk:\n  index: '{{ index }}'",
            "type: opensearch\nendpoints: [http://localhost:9200]\nopensearch_service_type: serverless",
        ] {
            let config: BoxedSink = serde_yaml::from_str(yaml).unwrap();
            assert!(config.validate_dyn().is_err());
        }
    }

    #[test]
    fn rejects_unknown_options() {
        assert!(
            serde_yaml::from_str::<BoxedSink>(
                "type: opensearch\nendpoints: [http://localhost:9200]\nunknown_option: true"
            )
            .is_err()
        );
    }

    #[cfg(feature = "aws-core")]
    #[test]
    fn serverless_requires_aws_auth_and_automatic_api_detection() {
        let yaml = r"
            type: opensearch
            endpoints: [https://collection.us-east-1.aoss.amazonaws.com]
            opensearch_service_type: serverless
            auth:
              strategy: aws
            aws:
              region: us-east-1
        ";
        let config: BoxedSink = serde_yaml::from_str(yaml).unwrap();
        config.validate_dyn().unwrap();

        let mut value = serde_json::to_value(config).unwrap();
        value["api_version"] = serde_json::json!("v7");
        let config: BoxedSink = serde_json::from_value(value).unwrap();
        assert!(config.validate_dyn().is_err());
    }
}
