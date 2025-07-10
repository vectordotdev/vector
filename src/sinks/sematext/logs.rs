use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use indoc::indoc;
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;
use vrl::event_path;

use super::Region;
use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    event::EventArray,
    sinks::{
        elasticsearch::{BulkConfig, ElasticsearchApiVersion, ElasticsearchConfig},
        util::{
            http::RequestConfig, BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings,
            StreamSink, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::Template,
};

/// Configuration for the `sematext_logs` sink.
#[configurable_component(sink("sematext_logs", "Publish log events to Sematext."))]
#[derive(Clone, Debug)]
pub struct SematextLogsConfig {
    #[serde(default = "super::default_region")]
    #[configurable(derived)]
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

    #[configurable(derived)]
    #[serde(skip_serializing_if = "crate::serde::is_default", default)]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for SematextLogsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            token = "${SEMATEXT_TOKEN}"
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
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = match (&self.endpoint, &self.region) {
            (Some(endpoint), _) => endpoint.clone(),
            (None, Region::Us) => US_ENDPOINT.to_owned(),
            (None, Region::Eu) => EU_ENDPOINT.to_owned(),
        };

        let (sink, healthcheck) = ElasticsearchConfig {
            endpoints: vec![endpoint],
            compression: Compression::None,
            doc_type: "\
                logs"
                .to_string(),
            bulk: BulkConfig {
                index: Template::try_from(self.token.inner())
                    .expect("unable to parse token as Template"),
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
        }
        .build(cx)
        .await?;

        let stream = sink.into_stream();
        let mapped_stream = MapTimestampStream { inner: stream };

        Ok((VectorSink::Stream(Box::new(mapped_stream)), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
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
        config::SinkConfig,
        sinks::util::test::{build_test_server, load_sink},
        test_util::{
            components::{self, HTTP_SINK_TAGS},
            next_addr, random_lines_with_stream,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SematextLogsConfig>();
    }

    #[tokio::test]
    async fn smoke() {
        let (mut config, cx) = load_sink::<SematextLogsConfig>(indoc! {r#"
            token = "mylogtoken"
        "#})
        .unwrap();

        // Make sure we can build the config
        _ = config.build(cx.clone()).await.unwrap();

        let addr = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        config.endpoint = Some(format!("http://{}", addr));

        let (sink, _) = config.build(cx).await.unwrap();

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
