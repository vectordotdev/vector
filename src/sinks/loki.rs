//! Loki sink
//!
//! This sink provides downstream support for `Loki` via
//! the v1 http json endpoint.
//!
//! https://github.com/grafana/loki/blob/master/docs/api.md
//!
//! This sink does not use `PartitionBatching` but elects to do
//! stream multiplexing by organizing the streams in the `build_request`
//! phase. There must be at least one valid set of labels.
//!
//! If an event produces no labels, this can happen if the template
//! does not match, we will add a default label `{agent="vector"}`.

use crate::{
    dns::Resolver,
    event::{self, Event, Value},
    runtime::FutureExt,
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::{Auth, BatchedHttpSink, HttpClient, HttpSink},
        service2::TowerRequestConfig,
        BatchBytesConfig, UriSerde,
    },
    template::Template,
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use derivative::Derivative;
use futures01::Sink;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

type Labels = Vec<(String, String)>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LokiConfig {
    endpoint: UriSerde,
    #[serde(default)]
    encoding: EncodingConfigWithDefault<Encoding>,

    tenant_id: Option<String>,
    labels: HashMap<String, Template>,

    #[serde(default = "crate::serde::default_false")]
    remove_label_fields: bool,
    #[serde(default = "crate::serde::default_true")]
    remove_timestamp: bool,

    auth: Option<Auth>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchBytesConfig,

    tls: Option<TlsOptions>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
enum Encoding {
    #[derivative(Default)]
    Json,
    Text,
}

inventory::submit! {
    SinkDescription::new_without_default::<LokiConfig>("loki")
}

#[typetag::serde(name = "loki")]
impl SinkConfig for LokiConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        if self.labels.is_empty() {
            return Err(format!("`labels` must include at least one label.").into());
        }

        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = self.batch.unwrap_or(bytesize::mib(10u64), 1);
        let tls = TlsSettings::from_options(&self.tls)?;

        let sink = BatchedHttpSink::new(
            self.clone(),
            Vec::new(),
            request_settings,
            batch_settings,
            Some(tls),
            &cx,
        )
        .sink_map_err(|e| error!("Fatal loki sink error: {}", e));

        let healthcheck = healthcheck(self.clone(), cx.resolver()).boxed_compat();

        Ok((Box::new(sink), Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "loki"
    }
}

#[async_trait::async_trait]
impl HttpSink for LokiConfig {
    type Input = (Labels, (i64, String));
    type Output = Vec<(Labels, (i64, String))>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);
        let mut labels = Vec::new();

        for (key, template) in &self.labels {
            if let Ok(value) = template.render_string(&event) {
                labels.push((key.clone(), value));
            }

            if self.remove_label_fields {
                if let Some(fields) = template.get_fields() {
                    for field in fields {
                        event.as_mut_log().remove(&field);
                    }
                }
            }
        }

        let ts = if let Some(event::Value::Timestamp(ts)) =
            event.as_log().get(&event::log_schema().timestamp_key())
        {
            ts.timestamp_nanos()
        } else {
            chrono::Utc::now().timestamp_nanos()
        };

        if self.remove_timestamp {
            event
                .as_mut_log()
                .remove(&event::log_schema().timestamp_key());
        }

        let event = match &self.encoding.codec() {
            Encoding::Json => serde_json::to_string(&event.as_log().all_fields())
                .expect("json encoding should never fail"),

            Encoding::Text => event
                .as_log()
                .get(&event::log_schema().message_key())
                .map(Value::to_string_lossy)
                .unwrap_or_default(),
        };

        // If no labels are provided we set our own default
        // `{agent="vector"}` label. This can happen if the only
        // label is a templatable one but the event doesn't match.
        if labels.is_empty() {
            labels = vec![("agent".to_string(), "vector".to_string())]
        }

        Some((labels, (ts, event)))
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let mut streams: HashMap<Labels, Vec<(i64, String)>> = HashMap::new();

        for (mut labels, event) in events {
            // We must sort here to ensure it hashes to the same stream
            // if the label set matches.
            labels.sort();

            if let Some(stream) = streams.get_mut(&labels) {
                stream.push(event);
            } else {
                streams.insert(labels, vec![event]);
            }
        }

        // Construct the json body
        let mut streams_json: Vec<serde_json::Value> = Vec::new();

        for (stream, mut events) in streams {
            // Sort by timestamp
            events.sort_by_key(|e| e.0);

            let stream = stream.into_iter().collect::<HashMap<_, _>>();
            let events = events
                .into_iter()
                // The final json output should be: `[ts, line]`
                .map(|e| json!([format!("{}", e.0), e.1]))
                .collect::<Vec<_>>();

            streams_json.push(json!({
                "stream": stream,
                "values": events,
            }));
        }

        let body = serde_json::to_vec(&json!({
            "streams": streams_json,
        }))
        .unwrap();

        let uri = format!("{}loki/api/v1/push", self.endpoint);

        let mut req = http::Request::post(uri).header("Content-Type", "application/json");

        if let Some(tenant_id) = &self.tenant_id {
            req = req.header("X-Scope-OrgID", tenant_id);
        }

        let mut req = req.body(body).unwrap();

        if let Some(auth) = &self.auth {
            auth.apply(&mut req);
        }

        Ok(req)
    }
}

async fn healthcheck(config: LokiConfig, resolver: Resolver) -> crate::Result<()> {
    let uri = format!("{}ready", config.endpoint);

    let tls = TlsSettings::from_options(&config.tls)?;
    let mut client = HttpClient::new(resolver, tls)?;

    let req = http::Request::get(uri).body(hyper::Body::empty()).unwrap();

    let res = client.send(req).await?;

    if res.status() != http::StatusCode::OK {
        return Err(format!("A non-successful status returned: {}", res.status()).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::http::HttpSink;
    use crate::sinks::util::test::load_sink;
    use crate::Event;

    #[test]
    fn interpolate_labels() {
        let (config, _cx, _rt) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {label1 = "{{ foo }}", label2 = "some-static-label"}
            encoding = "json"
            remove_label_fields = true
        "#,
        )
        .unwrap();

        let mut e1 = Event::from("hello world");

        e1.as_mut_log().insert("foo", "bar");

        let (mut labels, (_, line)) = config.encode_event(e1).unwrap();

        // HashMap -> Vec doesn't like keeping ordering
        labels.sort();

        // The final event should have timestamps and labels removed
        let expected_line = serde_json::to_string(&serde_json::json!({
            "message": "hello world",
        }))
        .unwrap();

        assert_eq!(line, expected_line);

        assert_eq!(labels[0], ("label1".to_string(), "bar".to_string()));
        assert_eq!(
            labels[1],
            ("label2".to_string(), "some-static-label".to_string())
        );
    }
}

#[cfg(feature = "docker")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::sinks::util::test::load_sink;
    use crate::template::Template;
    use crate::topology::config::SinkConfig;
    use crate::Event;
    use bytes::Bytes;
    use futures01::Sink;
    use std::convert::TryFrom;

    #[test]
    fn text() {
        let stream = uuid::Uuid::new_v4();

        let (mut config, cx, mut rt) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "placeholder"}
            encoding = "text"
        "#,
        )
        .unwrap();

        let test_name = config.labels.get_mut("test_name").unwrap();
        assert_eq!(test_name.get_ref(), &Bytes::from("placeholder"));

        *test_name = Template::try_from(stream.to_string()).unwrap();

        let (sink, _) = config.build(cx).unwrap();

        let lines = crate::test_util::random_lines(100)
            .take(10)
            .collect::<Vec<_>>();

        let events = lines.clone().into_iter().map(Event::from);
        let _ = rt
            .block_on(sink.send_all(futures01::stream::iter_ok(events)))
            .unwrap();

        let outputs = fetch_stream(stream.to_string());

        for (i, output) in outputs.iter().enumerate() {
            assert_eq!(output, &lines[i]);
        }
    }

    #[test]
    fn json() {
        let stream = uuid::Uuid::new_v4();

        let (mut config, cx, mut rt) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "placeholder"}
            encoding = "json"
            remove_timestamp = false
        "#,
        )
        .unwrap();

        let test_name = config.labels.get_mut("test_name").unwrap();
        assert_eq!(test_name.get_ref(), &Bytes::from("placeholder"));

        *test_name = Template::try_from(stream.to_string()).unwrap();

        let (sink, _) = config.build(cx).unwrap();

        let events = crate::test_util::random_lines(100)
            .take(10)
            .map(Event::from)
            .collect::<Vec<_>>();

        let _ = rt
            .block_on(sink.send_all(futures01::stream::iter_ok(events.clone())))
            .unwrap();

        let outputs = fetch_stream(stream.to_string());

        for (i, output) in outputs.iter().enumerate() {
            let expected_json = serde_json::to_string(&events[i].as_log().all_fields()).unwrap();
            assert_eq!(output, &expected_json);
        }
    }

    #[test]
    fn many_streams() {
        let stream1 = uuid::Uuid::new_v4();
        let stream2 = uuid::Uuid::new_v4();

        let (config, cx, mut rt) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "{{ stream_id }}"}
            encoding = "text"
        "#,
        )
        .unwrap();

        let (sink, _) = config.build(cx).unwrap();

        let lines = crate::test_util::random_lines(100)
            .take(10)
            .collect::<Vec<_>>();

        let mut events = lines
            .clone()
            .into_iter()
            .map(Event::from)
            .collect::<Vec<_>>();

        for i in 0..10 {
            let event = events.get_mut(i).unwrap();

            if i % 2 == 0 {
                event.as_mut_log().insert("stream_id", stream1.to_string());
            } else {
                event.as_mut_log().insert("stream_id", stream2.to_string());
            }
        }

        let _ = rt
            .block_on(sink.send_all(futures01::stream::iter_ok(events)))
            .unwrap();

        let outputs1 = fetch_stream(stream1.to_string());
        let outputs2 = fetch_stream(stream2.to_string());

        for (i, output) in outputs1.iter().enumerate() {
            let index = (i % 5) * 2;
            assert_eq!(output, &lines[index]);
        }

        for (i, output) in outputs2.iter().enumerate() {
            let index = ((i % 5) * 2) + 1;
            assert_eq!(output, &lines[index]);
        }
    }

    fn fetch_stream(stream: String) -> Vec<String> {
        let query = format!("%7Btest_name%3D\"{}\"%7D", stream);
        let query = format!(
            "http://localhost:3100/loki/api/v1/query_range?query={}&direction=forward",
            query
        );
        let mut res = reqwest::get(&query).unwrap();

        assert_eq!(res.status(), 200);

        // The response type follows this api https://github.com/grafana/loki/blob/master/docs/api.md#get-lokiapiv1query_range
        // where the result type is `streams`.
        let data = res.json::<serde_json::Value>().unwrap();

        // TODO: clean this up or explain it via docs
        let results = data
            .get("data")
            .unwrap()
            .get("result")
            .unwrap()
            .as_array()
            .unwrap();

        let values = results[0].get("values").unwrap().as_array().unwrap();

        values
            .iter()
            // Lets check the message field of the array where
            // the array looks like: [ts, line].
            .map(|v| v[1].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    }
}
