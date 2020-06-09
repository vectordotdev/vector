use crate::{
    sinks::elasticsearch::{ElasticSearchConfig, Encoding},
    sinks::util::{
        encoding::EncodingConfigWithDefault, service2::TowerRequestConfig, BatchBytesConfig,
        Compression,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
    Event,
};
use futures01::{Future, Sink};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SematextLogsConfig {
    region: Option<Region>,
    // TODO: replace this with `UriEncode` once that is on master.
    host: Option<String>,
    token: String,

    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchBytesConfig,
}

inventory::submit! {
    SinkDescription::new_without_default::<SematextLogsConfig>("sematext")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Na,
    Eu,
}

#[typetag::serde(name = "sematext")]
impl SinkConfig for SematextLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let host = match (&self.host, &self.region) {
            (Some(host), None) => host.clone(),
            (None, Some(Region::Na)) => "https://logsene-receiver.sematext.com".to_string(),
            (None, Some(Region::Eu)) => "https://logsene-receiver.eu.sematext.com".to_string(),
            (None, None) => {
                return Err(format!("Either `region` or `host` must be set.").into());
            }
            (Some(_), Some(_)) => {
                return Err(format!("Only one of `region` and `host` can be set.").into());
            }
        };

        let (sink, healthcheck) = ElasticSearchConfig {
            host,
            compression: Compression::None,
            doc_type: Some("logs".to_string()),
            index: Some(self.token.clone()),
            batch: self.batch.clone(),
            request: self.request.clone(),
            encoding: self.encoding.clone(),
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

    if let Some(ts) = log.remove(&crate::event::log_schema().timestamp_key()) {
        log.insert("@timestamp", ts);
    }

    if let Some(host) = log.remove(&crate::event::log_schema().host_key()) {
        log.insert("os.host", host);
    }

    futures01::future::ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use crate::sinks::util::test::{build_test_server, load_sink};
    use crate::test_util;
    use crate::topology::config::SinkConfig;
    use futures01::{Sink, Stream};

    #[test]
    fn smoke() {
        let (mut config, cx, mut rt) = load_sink::<SematextLogsConfig>(
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
        config.region = None;

        let (sink, _) = config.build(cx).unwrap();

        let (rx, _trigger, server) = build_test_server(addr, &mut rt);
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
