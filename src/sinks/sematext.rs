use crate::{
    sinks::elasticsearch::ElasticSearchConfig,
    sinks::util::Compression,
    topology::config::{DataType, SinkConfig, SinkContext},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct SematextConfig {
    cloud: Cloud,
    #[serde(flatten)]
    inner: ElasticSearchConfig,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cloud {
    NorthAmerica,
    Europe,
}

#[typetag::serde(name = "sematext")]
impl SinkConfig for SematextConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let mut es_config = self.inner.clone();

        let host = match &self.cloud {
            Cloud::NorthAmerica => "https://logsene-receiver.sematext.com",
            Cloud::Europe => "https://logsene-receiver.sematext.com",
        };

        // Custom config overrides
        if es_config.host.is_none() {
            es_config.host = Some(host.to_string());
        }

        if es_config.index.is_none() {
            Err(format!("`index` field is required"))?;
        }

        if es_config.doc_type.is_none() {
            es_config.doc_type = Some("logs".to_string());
        }

        if es_config.compression.is_none() {
            es_config.compression = Some(Compression::None);
        }

        es_config.build(cx)
    }

    fn input_type(&self) -> DataType {
        self.inner.input_type()
    }

    fn sink_type(&self) -> &'static str {
        "sematext"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use crate::sinks::util::test::{build_test_server, load_sink};
    use crate::test_util;
    use crate::topology::config::SinkConfig;
    use futures::{Sink, Stream};

    #[test]
    fn smoke() {
        let (mut config, cx, mut rt) = load_sink::<SematextConfig>(
            r#"
           cloud = "north_america"
            index = "mylogtoken"
        "#,
        )
        .unwrap();

        // Make sure we can build the config
        let _ = config.build(cx.clone()).unwrap();

        let addr = test_util::next_addr();
        // Swap out the host so we can force send it
        // to our local server
        config.inner.host = Some(format!("http://{}", addr));

        let (sink, _) = config.build(cx).unwrap();

        let (rx, _trigger, server) = build_test_server(&addr);
        rt.spawn(server);

        let (expected, lines) = test_util::random_lines_with_stream(100, 10);
        let pump = sink.send_all(lines.map(Event::from));
        let _ = rt.block_on(pump).unwrap();

        let output = rx.take(1).wait().collect::<Result<Vec<_>, _>>().unwrap();

        let val = serde_json::Deserializer::from_slice(&output[0].1[..])
            .into_iter::<serde_json::Value>()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|v| v.1.unwrap())
            .map(|v| v.get("message").unwrap().as_str().unwrap().to_string())
            .enumerate()
            .collect::<Vec<_>>();

        for (i, val) in val {
            assert_eq!(expected[i], val);
        }
    }
}
