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
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::{self, Event, Value},
    sinks::util::{
        buffer::loki::{LokiBuffer, LokiEvent, LokiRecord},
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::{Auth, BatchedHttpSink, HttpClient, HttpSink},
        BatchConfig, BatchSettings, TowerRequestConfig, UriSerde,
    },
    template::Template,
    tls::{TlsOptions, TlsSettings},
};
use derivative::Derivative;
use futures::FutureExt;
use futures01::Sink;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

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
    batch: BatchConfig,

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
    SinkDescription::new::<LokiConfig>("loki")
}

impl GenerateConfig for LokiConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "loki")]
impl SinkConfig for LokiConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        if self.labels.is_empty() {
            return Err("`labels` must include at least one label.".into());
        }

        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = BatchSettings::default()
            .bytes(102_400)
            .events(100_000)
            .timeout(1)
            .parse_config(self.batch)?;
        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(cx.resolver(), tls)?;

        let sink = BatchedHttpSink::new(
            self.clone(),
            LokiBuffer::new(batch_settings.size),
            request_settings,
            batch_settings.timeout,
            client.clone(),
            cx.acker(),
        )
        .sink_map_err(|e| error!("Fatal loki sink error: {}", e));

        let healthcheck = healthcheck(self.clone(), client).boxed();

        Ok((
            super::VectorSink::Futures01Sink(Box::new(sink)),
            healthcheck,
        ))
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
    type Input = LokiRecord;
    type Output = serde_json::Value;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
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

        let timestamp = match event
            .as_log()
            .get(&Atom::from(log_schema().timestamp_key()))
        {
            Some(event::Value::Timestamp(ts)) => ts.timestamp_nanos(),
            _ => chrono::Utc::now().timestamp_nanos(),
        };

        if self.remove_timestamp {
            event
                .as_mut_log()
                .remove(&Atom::from(log_schema().timestamp_key()));
        }

        self.encoding.apply_rules(&mut event);
        let event = match &self.encoding.codec() {
            Encoding::Json => serde_json::to_string(&event.as_log().all_fields())
                .expect("json encoding should never fail"),

            Encoding::Text => event
                .as_log()
                .get(&Atom::from(log_schema().message_key()))
                .map(Value::to_string_lossy)
                .unwrap_or_default(),
        };

        // If no labels are provided we set our own default
        // `{agent="vector"}` label. This can happen if the only
        // label is a templatable one but the event doesn't match.
        if labels.is_empty() {
            labels = vec![("agent".to_string(), "vector".to_string())]
        }

        let event = LokiEvent { timestamp, event };
        Some(LokiRecord { labels, event })
    }

    async fn build_request(&self, json: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let body = serde_json::to_vec(&json).unwrap();

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

async fn healthcheck(config: LokiConfig, mut client: HttpClient) -> crate::Result<()> {
    let uri = format!("{}ready", config.endpoint);

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
        let (config, _cx) = load_sink::<LokiConfig>(
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

        let mut record = config.encode_event(e1).unwrap();

        // HashMap -> Vec doesn't like keeping ordering
        record.labels.sort();

        // The final event should have timestamps and labels removed
        let expected_line = serde_json::to_string(&serde_json::json!({
            "message": "hello world",
        }))
        .unwrap();

        assert_eq!(record.event.event, expected_line);

        assert_eq!(record.labels[0], ("label1".to_string(), "bar".to_string()));
        assert_eq!(
            record.labels[1],
            ("label2".to_string(), "some-static-label".to_string())
        );
    }

    #[test]
    fn use_label_from_dropped_fields() {
        let (config, _cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels.bar = "{{ foo }}"
            encoding.codec = "json"
            encoding.except_fields = ["foo"]
        "#,
        )
        .unwrap();

        let mut e1 = Event::from("hello world");

        e1.as_mut_log().insert("foo", "bar");

        let record = config.encode_event(e1).unwrap();

        let expected_line = serde_json::to_string(&serde_json::json!({
            "message": "hello world",
        }))
        .unwrap();

        assert_eq!(record.event.event, expected_line);

        assert_eq!(record.labels[0], ("bar".to_string(), "bar".to_string()));
    }
}

#[cfg(feature = "loki-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        config::SinkConfig, sinks::util::test::load_sink, template::Template,
        test_util::random_lines, Event,
    };
    use bytes::Bytes;
    use futures::compat::Future01CompatExt;
    use futures01::Sink;
    use std::convert::TryFrom;

    #[tokio::test]
    async fn text() {
        let stream = uuid::Uuid::new_v4();

        let (mut config, cx) = load_sink::<LokiConfig>(
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

        let (sink, _) = config.build(cx).await.unwrap();

        let lines = random_lines(100).take(10).collect::<Vec<_>>();

        let events = lines.clone().into_iter().map(Event::from);
        let _ = sink
            .into_futures01sink()
            .send_all(futures01::stream::iter_ok(events))
            .compat()
            .await
            .unwrap();

        let outputs = fetch_stream(stream.to_string()).await;
        for (i, output) in outputs.iter().enumerate() {
            assert_eq!(output, &lines[i]);
        }
    }

    #[tokio::test]
    async fn json() {
        let stream = uuid::Uuid::new_v4();

        let (mut config, cx) = load_sink::<LokiConfig>(
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

        let (sink, _) = config.build(cx).await.unwrap();

        let events = random_lines(100)
            .take(10)
            .map(Event::from)
            .collect::<Vec<_>>();
        let _ = sink
            .into_futures01sink()
            .send_all(futures01::stream::iter_ok(events.clone()))
            .compat()
            .await
            .unwrap();

        let outputs = fetch_stream(stream.to_string()).await;
        for (i, output) in outputs.iter().enumerate() {
            let expected_json = serde_json::to_string(&events[i].as_log().all_fields()).unwrap();
            assert_eq!(output, &expected_json);
        }
    }

    #[tokio::test]
    async fn many_streams() {
        let stream1 = uuid::Uuid::new_v4();
        let stream2 = uuid::Uuid::new_v4();

        let (config, cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "{{ stream_id }}"}
            encoding = "text"
        "#,
        )
        .unwrap();

        let (sink, _) = config.build(cx).await.unwrap();

        let lines = random_lines(100).take(10).collect::<Vec<_>>();

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

        let _ = sink
            .into_futures01sink()
            .send_all(futures01::stream::iter_ok(events))
            .compat()
            .await
            .unwrap();

        let outputs1 = fetch_stream(stream1.to_string()).await;
        let outputs2 = fetch_stream(stream2.to_string()).await;

        for (i, output) in outputs1.iter().enumerate() {
            let index = (i % 5) * 2;
            assert_eq!(output, &lines[index]);
        }

        for (i, output) in outputs2.iter().enumerate() {
            let index = ((i % 5) * 2) + 1;
            assert_eq!(output, &lines[index]);
        }
    }

    async fn fetch_stream(stream: String) -> Vec<String> {
        let query = format!("%7Btest_name%3D\"{}\"%7D", stream);
        let query = format!(
            "http://localhost:3100/loki/api/v1/query_range?query={}&direction=forward",
            query
        );
        let res = reqwest::get(&query).await.unwrap();

        assert_eq!(res.status(), 200);

        // The response type follows this api https://github.com/grafana/loki/blob/master/docs/api.md#get-lokiapiv1query_range
        // where the result type is `streams`.
        let data = res.json::<serde_json::Value>().await.unwrap();

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
