use vector_lib::configurable::configurable_component;

use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    sinks::{
        Healthcheck, VectorSink,
        elasticsearch::{Distribution, ElasticsearchConfig},
    },
    template::ConfinementConfig,
};

/// Configuration for the `opensearch` sink.
///
/// Uses the Elasticsearch-compatible bulk API supported by OpenSearch. Configuration options
/// and delivery behavior are shared with the `elasticsearch` sink. The default API version
/// is selected for OpenSearch, including Amazon OpenSearch Service and OpenSearch Serverless.
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
        self.0.validate_with_distribution(Distribution::OpenSearch)
    }

    async fn build(
        &self,
        validated: &Self::Validated,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        self.0
            .build_with_distribution(validated, cx, Distribution::OpenSearch)
            .await
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

    #[tokio::test]
    async fn confinement_warnings_use_actual_sink_type() {
        use std::io::{Read, Seek, SeekFrom};

        use tracing::instrument::WithSubscriber;

        for sink_type in ["elasticsearch", "opensearch"] {
            for (mode, expected_fields) in [
                ("bulk", vec!["bulk.index"]),
                (
                    "data_stream",
                    vec![
                        "data_stream.type",
                        "data_stream.dataset",
                        "data_stream.namespace",
                    ],
                ),
            ] {
                let config: BoxedSink = serde_yaml::from_str(&format!(
                    "type: {sink_type}\nendpoints: [http://localhost:9200]\napi_version: v7\n\
                     mode: {mode}\ndangerously_allow_unconfined_template_resolution: true"
                ))
                .unwrap();
                let mut logs = tempfile::tempfile().unwrap();
                let subscriber = tracing::Dispatch::new(
                    tracing_subscriber::fmt()
                        .json()
                        .with_max_level(tracing::Level::WARN)
                        .with_writer(logs.try_clone().unwrap())
                        .finish(),
                );
                let validated = tracing::dispatcher::with_default(&subscriber, || {
                    config.validate_dyn().unwrap()
                });
                let validation_end = logs.stream_position().unwrap();
                // An explicit API version avoids network I/O; the healthcheck is not polled.
                let (_sink, _healthcheck) = config
                    .build_dyn(validated.as_ref(), SinkContext::default())
                    .with_subscriber(subscriber)
                    .await
                    .unwrap();
                logs.seek(SeekFrom::Start(0)).unwrap();
                let mut output = String::new();
                logs.read_to_string(&mut output).unwrap();
                let (validation_logs, build_logs) =
                    output.split_at(validation_end.try_into().unwrap());
                for phase_logs in [validation_logs, build_logs] {
                    let warnings: Vec<serde_json::Value> = phase_logs
                        .lines()
                        .map(|line| serde_json::from_str(line).unwrap())
                        .filter(|event: &serde_json::Value| {
                            event["fields"]["message"]
                                .as_str()
                                .is_some_and(|message| message.starts_with("SECURITY:"))
                        })
                        .collect();
                    assert_eq!(warnings.len(), expected_fields.len(), "{phase_logs}");
                    for (warning, field) in warnings.iter().zip(&expected_fields) {
                        assert_eq!(warning["fields"]["component_kind"], "sink");
                        assert_eq!(warning["fields"]["component_type"], sink_type);
                        assert_eq!(warning["fields"]["field"], *field);
                    }
                }
            }
        }
    }

    #[test]
    fn defaults_to_managed_with_auto_api_version() {
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
