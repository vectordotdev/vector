use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use indoc::indoc;
use serde::{Deserialize, Serialize};

use super::Region;
use crate::sinks::elasticsearch::BulkConfig;
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::EventArray,
    sinks::{
        elasticsearch::{ElasticSearchConfig, ElasticSearchEncoder},
        util::{
            encoding::EncodingConfigFixed, http::RequestConfig, BatchConfig, Compression,
            RealtimeSizeBasedDefaultBatchSettings, StreamSink, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SematextLogsConfig {
    region: Option<Region>,
    // Deprecated name
    #[serde(alias = "host")]
    endpoint: Option<String>,
    token: String,

    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigFixed<ElasticSearchEncoder>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
}

inventory::submit! {
    SinkDescription::new::<SematextLogsConfig>("sematext_logs")
}

impl GenerateConfig for SematextLogsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            region = "us"
            token = "${SEMATEXT_TOKEN}"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "sematext_logs")]
impl SinkConfig for SematextLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = match (&self.endpoint, &self.region) {
            (Some(host), None) => host.clone(),
            (None, Some(Region::Us)) => "https://logsene-receiver.sematext.com".to_owned(),
            (None, Some(Region::Eu)) => "https://logsene-receiver.eu.sematext.com".to_owned(),
            (None, None) => "https://logsene-receiver.sematext.com".to_owned(),
            (Some(_), Some(_)) => {
                return Err("Only one of `region` and `host` can be set.".into());
            }
        };

        let (sink, healthcheck) = ElasticSearchConfig {
            endpoint,
            compression: Compression::None,
            doc_type: Some(
                "\
            logs"
                    .to_string(),
            ),
            bulk: Some(BulkConfig {
                action: None,
                index: Some(self.token.clone()),
            }),
            batch: self.batch,
            request: RequestConfig {
                tower: self.request,
                ..Default::default()
            },
            encoding: self.encoding.clone(),
            ..Default::default()
        }
        .build(cx)
        .await?;

        let stream = sink.into_stream();
        let mapped_stream = MapTimestampStream { inner: stream };

        Ok((VectorSink::Stream(Box::new(mapped_stream)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "sematext_logs"
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
                if let Some(ts) = log.remove(crate::config::log_schema().timestamp_key()) {
                    log.insert("@timestamp", ts);
                }

                if let Some(host) = log.remove(crate::config::log_schema().host_key()) {
                    log.insert("os.host", host);
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
            region = "us"
            token = "mylogtoken"
        "#})
        .unwrap();

        // Make sure we can build the config
        let _ = config.build(cx.clone()).await.unwrap();

        let addr = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        config.endpoint = Some(format!("http://{}", addr));
        config.region = None;

        let (sink, _) = config.build(cx).await.unwrap();

        let (mut rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let (expected, events) = random_lines_with_stream(100, 10, None);
        components::run_sink(sink, events, &HTTP_SINK_TAGS).await;

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
