use crate::{
    sinks::elasticsearch::ElasticSearchConfig,
    sinks::util::{BatchBytesConfig, Compression, TowerRequestConfig},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
    Event,
};
use futures::{Future, Sink};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SematextConfig {
    region: Option<Region>,
    // TODO: replace this with `UriEncode` once that is on master.
    host: Option<String>,
    token: String,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchBytesConfig,
}

inventory::submit! {
    SinkDescription::new_without_default::<SematextConfig>("sematext")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Na,
    Eu,
}

#[typetag::serde(name = "sematext")]
impl SinkConfig for SematextConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let mut host = None;

        if let Some(region) = &self.region {
            host = match &region {
                Region::Na => "https://logsene-receiver.sematext.com".to_string().into(),
                Region::Eu => "https://logsene-receiver.eu.sematext.com"
                    .to_string()
                    .into(),
            };
        }

        // Test workaround for settings a custom host so we can test the body manually
        if let Some(h) = &self.host {
            host = Some(h.clone());
        }

        if host.is_none() {
            return Err(format!("Either `region` or `host` must be set.").into());
        }

        let (sink, healthcheck) = ElasticSearchConfig {
            host,
            compression: Some(Compression::None),
            doc_type: Some("logs".to_string()),
            index: Some(self.token.clone()),
            batch: self.batch.clone(),
            request: self.request.clone(),
            ..Default::default()
        }
        .build(cx)?;

        let sink = Box::new(sink.with(map_timestamp));

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "sematext"
    }
}

/// Used to map `timestamp` to `@timestamp`.
fn map_timestamp(mut event: Event) -> impl Future<Item = Event, Error = ()> {
    let log = event.as_mut_log();

    if let Some(ts) = log.remove(&crate::event::TIMESTAMP) {
        log.insert("@timestamp", ts);
    }

    if let Some(host) = log.remove(&crate::event::HOST) {
        log.insert("os.host", host);
    }

    futures::future::ok(event)
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
            region = "na"
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

        // A stream of `serde_json::Value`
        let json = serde_json::Deserializer::from_slice(&output[0].1[..])
            .into_iter::<serde_json::Value>()
            .map(|v| v.expect("decoding json"));

        let mut expected_message_idx = 0;
        for (i, val) in json.enumerate() {
            // Every even message is the index which contains the token for sematext
            // Every odd message is the actual message in json format.
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
