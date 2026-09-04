#![expect(
    clippy::let_underscore_must_use,
    reason = "derivative's Debug derive with ignored fields expands to a must_use let binding"
)]

use async_trait::async_trait;
use derivative::Derivative;
use futures::stream::{BoxStream, StreamExt};
use indoc::indoc;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};
use vrl::event_path;

use super::Region;
use crate::{
    codecs::Transformer,
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    event::EventArray,
    sinks::{
        Healthcheck, VectorSink,
        elasticsearch::{BulkConfig, ElasticsearchApiVersion, ElasticsearchConfig},
        util::{
            BatchConfig, Compression, HttpEndpoint, RealtimeSizeBasedDefaultBatchSettings,
            StreamSink, TowerRequestConfig, http::RequestConfig,
        },
    },
    template::Template,
};

/// Configuration for the `sematext_logs` sink.
#[configurable_component(sink("sematext_logs", "Publish log events to Sematext."))]
#[derive(Clone, Debug)]
pub struct SematextLogsConfig {
    #[serde(default = "super::default_region")]
    region: Region,

    /// The endpoint to send data to.
    ///
    /// Setting this option overrides the `region` option.
    #[serde(alias = "host")]
    #[configurable(metadata(docs::examples = "http://127.0.0.1"))]
    #[configurable(metadata(docs::examples = "https://example.com"))]
    endpoint: Option<String>,

    /// The token that is used to write to Sematext.
    #[configurable(metadata(docs::examples = "${SEMATEXT_TOKEN}"))]
    #[configurable(metadata(docs::examples = "some-sematext-token"))]
    token: SensitiveString,

    #[serde(skip_serializing_if = "crate::serde::is_default", default)]
    pub encoding: Transformer,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for SematextLogsConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc! {r#"
            token: ${SEMATEXT_TOKEN}
        "#})
        .unwrap()
    }
}

// https://sematext.com/docs/logs/index-events-via-elasticsearch-api/
const US_ENDPOINT: &str = "https://logsene-receiver.sematext.com";
const EU_ENDPOINT: &str = "https://logsene-receiver.eu.sematext.com";

#[async_trait::async_trait]
#[typetag::serde(name = "sematext_logs")]
impl SinkConfig for SematextLogsConfig {
    fn input(&self) -> Input {
        Input::log()
    }
    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ValidatedSematextLogs {
    endpoint: String,
    // Omitted: `index` is built from the write token and would leak it via Debug.
    #[derivative(Debug = "ignore")]
    index: Template,
}

#[async_trait::async_trait]
impl ValidatedSink for SematextLogsConfig {
    type Validated = ValidatedSematextLogs;

    fn validate(&self) -> crate::Result<ValidatedSematextLogs> {
        let endpoint = match (&self.endpoint, &self.region) {
            (Some(endpoint), _) => endpoint.clone(),
            (None, Region::Us) => US_ENDPOINT.to_owned(),
            (None, Region::Eu) => EU_ENDPOINT.to_owned(),
        };

        let index = Template::try_from(self.token.inner())
            .map_err(|error| format!("unable to parse token as Template: {error}"))?;

        // Run the derived Elasticsearch config's full structural validation
        // (endpoints, batch, versioning, confinement) so a malformed endpoint
        // or token template is rejected here rather than at startup.
        self.derived_elasticsearch_config(&endpoint, &index)?
            .validate()?;

        Ok(ValidatedSematextLogs { endpoint, index })
    }

    async fn build(
        &self,
        validated: &ValidatedSematextLogs,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedSematextLogs { endpoint, index } = validated;

        let es_config = self.derived_elasticsearch_config(endpoint, index)?;
        let (sink, healthcheck) = SinkConfig::build(&es_config, cx).await?;

        let stream = sink.into_stream();
        let mapped_stream = MapTimestampStream { inner: stream };

        Ok((VectorSink::Stream(Box::new(mapped_stream)), healthcheck))
    }
}

impl SematextLogsConfig {
    /// Build the Elasticsearch config this sink delegates to.
    fn derived_elasticsearch_config(
        &self,
        endpoint: &str,
        index: &Template,
    ) -> crate::Result<ElasticsearchConfig> {
        let endpoint =
            HttpEndpoint::parse(endpoint).map_err(|e| format!("invalid Sematext endpoint: {e}"))?;
        Ok(ElasticsearchConfig {
            endpoints: vec![endpoint],
            compression: Compression::None,
            doc_type: "logs".to_string(),
            bulk: BulkConfig {
                index: index.clone(),
                ..Default::default()
            },
            batch: self.batch,
            request: RequestConfig {
                tower: self.request,
                ..Default::default()
            },
            encoding: self.encoding.clone(),
            api_version: ElasticsearchApiVersion::V6,
            ..Default::default()
        })
    }
}

struct MapTimestampStream {
    inner: Box<dyn StreamSink<EventArray> + Send>,
}

#[async_trait]
impl StreamSink<EventArray> for MapTimestampStream {
    async fn run(self: Box<Self>, input: BoxStream<'_, EventArray>) -> Result<(), ()> {
        let mapped_input = input.map(map_timestamp).boxed();
        self.inner.run(mapped_input).await
    }
}

/// Used to map `timestamp` to `@timestamp`.
fn map_timestamp(mut events: EventArray) -> EventArray {
    match &mut events {
        EventArray::Logs(logs) => {
            for log in logs {
                if let Some(path) = log.timestamp_path().cloned().as_ref() {
                    log.rename_key(path, event_path!("@timestamp"));
                }

                if let Some(path) = log.host_path().cloned().as_ref() {
                    log.rename_key(path, event_path!("os.host"));
                }
            }
        }
        _ => unreachable!("This sink only accepts logs"),
    }

    events
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use indoc::indoc;

    use super::*;
    use crate::{
        config::{SinkConfig, ValidatedSink},
        sinks::util::test::{build_test_server, load_sink},
        test_util::{
            addr::next_addr,
            components::{self, HTTP_SINK_TAGS},
            random_lines_with_stream,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SematextLogsConfig>();
    }

    #[test]
    fn prepares_valid_config() {
        let config = SematextLogsConfig {
            region: Region::Us,
            endpoint: None,
            token: "mylogtoken".to_string().into(),
            encoding: Default::default(),
            request: Default::default(),
            batch: Default::default(),
            acknowledgements: Default::default(),
        };

        let validated = config.validate().expect("preparation should succeed");
        assert_eq!(validated.endpoint, US_ENDPOINT);
        assert_eq!(validated.index.get_ref(), "mylogtoken");
    }

    #[test]
    fn debug_impl_does_not_leak_token() {
        // `ValidatedSematextLogs`'s `index` field is built from the write token,
        // so its Debug output must not expose the token value.
        let token = "mylogtoken";
        let config = SematextLogsConfig {
            region: Region::Us,
            endpoint: None,
            token: token.to_string().into(),
            encoding: Default::default(),
            request: Default::default(),
            batch: Default::default(),
            acknowledgements: Default::default(),
        };

        let validated = config.validate().expect("preparation should succeed");
        let debug = format!("{validated:?}");
        assert!(
            !debug.contains(token),
            "Debug output must not contain the write token, got: {debug}"
        );
    }

    #[test]
    fn validate_rejects_unconfined_token_template() {
        // A token that is an unconfined routing template (no literal prefix)
        // passes template parsing but is rejected by the derived Elasticsearch
        // config's confinement check. Structural validation must catch it here
        // rather than deferring the error to startup.
        let config = SematextLogsConfig {
            region: Region::Us,
            endpoint: None,
            token: "{{ index }}".to_string().into(),
            encoding: Default::default(),
            request: Default::default(),
            batch: Default::default(),
            acknowledgements: Default::default(),
        };

        assert!(
            config.validate().is_err(),
            "an unconfined token template should fail validation"
        );
    }

    #[test]
    fn validate_rejects_malformed_token_template() {
        // A token with invalid template syntax (a dangling `%` is an invalid
        // strftime item) must surface a validation error rather than panic
        // during structural validation.
        let config = SematextLogsConfig {
            region: Region::Us,
            endpoint: None,
            token: "%".to_string().into(),
            encoding: Default::default(),
            request: Default::default(),
            batch: Default::default(),
            acknowledgements: Default::default(),
        };

        let err = config
            .validate()
            .expect_err("a malformed token template should fail validation");
        assert!(
            err.to_string()
                .contains("unable to parse token as Template"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_malformed_endpoint() {
        // A custom endpoint that parses as a URI but has no host must be
        // rejected by the derived Elasticsearch config's structural validation
        // rather than failing at startup.
        let config = SematextLogsConfig {
            region: Region::Us,
            endpoint: Some("/path".to_string()),
            token: "mylogtoken".to_string().into(),
            encoding: Default::default(),
            request: Default::default(),
            batch: Default::default(),
            acknowledgements: Default::default(),
        };

        let err = config
            .validate()
            .expect_err("an endpoint without a host should fail validation");
        assert!(
            err.to_string().contains("invalid Sematext endpoint"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn smoke() {
        let (mut config, cx) = load_sink::<SematextLogsConfig>(indoc! {r#"
            token = "mylogtoken"
        "#})
        .unwrap();

        // Make sure we can build the config
        _ = SinkConfig::build(&config, cx.clone()).await.unwrap();

        let (_guard, addr) = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        config.endpoint = Some(format!("http://{addr}"));

        let (sink, _) = SinkConfig::build(&config, cx).await.unwrap();

        let (mut rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let (expected, events) = random_lines_with_stream(100, 10, None);
        components::run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

        let output = rx.next().await.unwrap();

        // A stream of `serde_json::Value`
        let json = serde_json::Deserializer::from_slice(&output.1[..])
            .into_iter::<serde_json::Value>()
            .map(|v| v.expect("decoding json"));

        let mut expected_message_idx = 0;
        for (i, val) in json.enumerate() {
            // Every even message is the index which contains the token for sematext
            // Every odd message is the actual message in JSON format.
            if i % 2 == 0 {
                // Fetch {index: {_index: ""}}
                let token = val
                    .get("index")
                    .unwrap()
                    .get("_index")
                    .unwrap()
                    .as_str()
                    .unwrap();

                assert_eq!(token, "mylogtoken");
            } else {
                let message = val.get("message").unwrap().as_str().unwrap();
                assert_eq!(message, &expected[expected_message_idx]);
                expected_message_idx += 1;
            }
        }
    }
}
