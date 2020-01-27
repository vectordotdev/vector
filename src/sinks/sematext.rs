use crate::{
    sinks::elasticsearch::ElasticSearchConfig,
    sinks::util::{BatchBytesConfig, Compression, TowerRequestConfig},
    topology::config::{DataType, SinkConfig, SinkContext},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SematextConfig {
    cloud: Cloud,
    token: String,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchBytesConfig,

    // Used for testing, `serde` will skip this field
    // and this can only be set manually once you
    // have a copy of this struct.
    #[serde(skip)]
    host: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cloud {
    NorthAmerica,
    Europe,
}

#[typetag::serde(name = "sematext")]
impl SinkConfig for SematextConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let mut host = match &self.cloud {
            Cloud::NorthAmerica => "https://logsene-receiver.sematext.com".to_string(),
            Cloud::Europe => "https://logsene-receiver.sematext.com".to_string(),
        };

        // Test workaround for settings a custom host so we can test the body manually
        if let Some(h) = &self.host {
            host = h.clone();
        }

        ElasticSearchConfig {
            host: Some(host),
            compression: Some(Compression::None),
            doc_type: Some("logs".to_string()),
            index: Some(self.token.clone()),
            batch: self.batch.clone(),
            request: self.request.clone(),
            ..Default::default()
        }
        .build(cx)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
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
            token = "mylogtoken"
        "#,
        )
        .unwrap();

        // Make sure we can build the config
        let _ = config.build(cx.clone()).unwrap();

        let addr = test_util::next_addr();
        // Swap out the host so we can force send it
        // to our local server
        config.host = Some(format!("http://{}", addr));

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
