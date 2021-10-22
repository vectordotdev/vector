//! Loki sink
//!
//! This sink provides downstream support for `Loki` via
//! the v1 http json endpoint.
//!
//! <https://github.com/grafana/loki/blob/master/docs/api.md>
//!
//! This sink uses `PartitionBatching` to partition events
//! by streams. There must be at least one valid set of labels.
//!
//! If an event produces no labels, this can happen if the template
//! does not match, we will add a default label `{agent="vector"}`.

use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::{self, Event, Value},
    http::{Auth, HttpClient, MaybeAuth},
    internal_events::{LokiEventUnlabeled, LokiEventsProcessed, TemplateRenderingFailed},
    sinks::util::{
        buffer::loki::{GlobalTimestamps, LokiBuffer, LokiEvent, LokiRecord, PartitionKey},
        encoding::{EncodingConfig, EncodingConfiguration},
        http::{HttpSink, PartitionHttpSink},
        BatchConfig, BatchSettings, PartitionBuffer, PartitionInnerBuffer, TowerRequestConfig,
        UriSerde,
    },
    template::Template,
    tls::{TlsOptions, TlsSettings},
};
use futures::{FutureExt, SinkExt};
use serde::{Deserialize, Serialize};
use shared::encode_logfmt;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LokiConfig {
    endpoint: UriSerde,
    encoding: EncodingConfig<Encoding>,

    tenant_id: Option<Template>,
    labels: HashMap<Template, Template>,

    #[serde(default = "crate::serde::default_false")]
    remove_label_fields: bool,
    #[serde(default = "crate::serde::default_true")]
    remove_timestamp: bool,
    #[serde(default)]
    out_of_order_action: OutOfOrderAction,

    auth: Option<Auth>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchConfig,

    tls: Option<TlsOptions>,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "snake_case")]
pub enum OutOfOrderAction {
    #[derivative(Default)]
    Drop,
    RewriteTimestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Encoding {
    Json,
    Text,
    Logfmt,
}

inventory::submit! {
    SinkDescription::new::<LokiConfig>("loki")
}

impl GenerateConfig for LokiConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "http://localhost:3100"
            encoding = "json"
            labels = {}"#,
        )
        .unwrap()
    }
}

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

        for label in self.labels.keys() {
            if !valid_label_name(label) {
                return Err(format!("Invalid label name {:?}", label.get_ref()).into());
            }
        }

        let request_settings = self.request.unwrap_with(&TowerRequestConfig {
            ..Default::default()
        });

        let batch_settings = BatchSettings::default()
            .bytes(102_400)
            .events(100_000)
            .timeout(1)
            .parse_config(self.batch)?;
        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;

        let config = LokiConfig {
            auth: self.auth.choose_one(&self.endpoint.auth)?,
            ..self.clone()
        };

        let sink = LokiSink::new(config.clone());

        let sink = PartitionHttpSink::new(
            sink,
            PartitionBuffer::new(LokiBuffer::new(
                batch_settings.size,
                GlobalTimestamps::default(),
                config.out_of_order_action.clone(),
            )),
            request_settings,
            batch_settings.timeout,
            client.clone(),
            cx.acker(),
        )
        .ordered()
        .sink_map_err(|error| error!(message = "Fatal loki sink error.", %error));

        let healthcheck = healthcheck(config, client).boxed();

        Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "loki"
    }
}

struct LokiSink {
    endpoint: UriSerde,
    encoding: EncodingConfig<Encoding>,

    tenant_id: Option<Template>,
    labels: HashMap<Template, Template>,

    remove_label_fields: bool,
    remove_timestamp: bool,

    auth: Option<Auth>,
}

impl LokiSink {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    fn new(config: LokiConfig) -> Self {
        Self {
            endpoint: config.endpoint,
            encoding: config.encoding,
            tenant_id: config.tenant_id,
            labels: config.labels,
            remove_label_fields: config.remove_label_fields,
            remove_timestamp: config.remove_timestamp,
            auth: config.auth,
        }
    }
}

#[async_trait::async_trait]
impl HttpSink for LokiSink {
    type Input = PartitionInnerBuffer<LokiRecord, PartitionKey>;
    type Output = PartitionInnerBuffer<serde_json::Value, PartitionKey>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        let tenant_id = self.tenant_id.as_ref().and_then(|t| {
            t.render_string(&event)
                .map_err(|error| {
                    emit!(&TemplateRenderingFailed {
                        error,
                        field: Some("tenant_id"),
                        drop_event: false,
                    })
                })
                .ok()
        });

        let mut labels = Vec::new();

        for (key_template, value_template) in &self.labels {
            if let (Ok(key), Ok(value)) = (
                key_template.render_string(&event),
                value_template.render_string(&event),
            ) {
                labels.push((key, value));
            }
        }

        if self.remove_label_fields {
            for template in self.labels.values() {
                if let Some(fields) = template.get_fields() {
                    for field in fields {
                        event.as_mut_log().remove(&field);
                    }
                }
            }
        }

        let timestamp = match event.as_log().get(log_schema().timestamp_key()) {
            Some(event::Value::Timestamp(ts)) => ts.timestamp_nanos(),
            _ => chrono::Utc::now().timestamp_nanos(),
        };

        if self.remove_timestamp {
            event.as_mut_log().remove(log_schema().timestamp_key());
        }

        self.encoding.apply_rules(&mut event);
        let log = event.into_log();
        let event = match &self.encoding.codec() {
            Encoding::Json => {
                serde_json::to_string(&log).expect("json encoding should never fail.")
            }

            Encoding::Text => log
                .get(log_schema().message_key())
                .map(Value::to_string_lossy)
                .unwrap_or_default(),

            Encoding::Logfmt => encode_logfmt::to_string(log.into_parts().0)
                .expect("Logfmt encoding should never fail."),
        };

        // If no labels are provided we set our own default
        // `{agent="vector"}` label. This can happen if the only
        // label is a templatable one but the event doesn't match.
        if labels.is_empty() {
            emit!(&LokiEventUnlabeled);
            labels = vec![("agent".to_string(), "vector".to_string())]
        }

        let key = PartitionKey::new(tenant_id, &mut labels);

        let event = LokiEvent { timestamp, event };
        Some(PartitionInnerBuffer::new(
            LokiRecord {
                labels,
                event,
                partition: key.clone(),
            },
            key,
        ))
    }

    async fn build_request(&self, output: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let (json, key) = output.into_parts();
        let tenant_id = key.tenant_id;

        let body = serde_json::to_vec(&json).unwrap();

        emit!(&LokiEventsProcessed {
            byte_size: body.len(),
        });

        let uri = format!("{}loki/api/v1/push", self.endpoint.uri);

        let mut req = http::Request::post(uri).header("Content-Type", "application/json");

        if let Some(tenant_id) = tenant_id {
            req = req.header("X-Scope-OrgID", tenant_id);
        }

        let mut req = req.body(body).unwrap();

        if let Some(auth) = &self.auth {
            auth.apply(&mut req);
        }

        Ok(req)
    }
}

async fn healthcheck(config: LokiConfig, client: HttpClient) -> crate::Result<()> {
    let uri = format!("{}ready", config.endpoint.uri);

    let mut req = http::Request::get(uri)
        .body(hyper::Body::empty())
        .expect("Building request never fails.");

    if let Some(auth) = &config.auth {
        auth.apply(&mut req);
    }

    let res = client.send(req).await?;

    let status = match fetch_status("ready", &config, &client).await? {
        // Issue https://github.com/timberio/vector/issues/6463
        http::StatusCode::NOT_FOUND => {
            debug!("Endpoint `/ready` not found. Retrying healthcheck with top level query.");
            fetch_status("", &config, &client).await?
        }
        status => status,
    };

    match status {
        http::StatusCode::OK => Ok(()),
        _ => Err(format!("A non-successful status returned: {}", res.status()).into()),
    }
}

async fn fetch_status(
    endpoint: &str,
    config: &LokiConfig,
    client: &HttpClient,
) -> crate::Result<http::StatusCode> {
    let uri = format!("{}{}", config.endpoint.uri, endpoint);

    let mut req = http::Request::get(uri).body(hyper::Body::empty()).unwrap();

    if let Some(auth) = &config.auth {
        auth.apply(&mut req);
    }

    Ok(client.send(req).await?.status())
}

fn valid_label_name(label: &Template) -> bool {
    label.is_dynamic() || {
        // Loki follows prometheus on this https://prometheus.io/docs/concepts/data_model/#metric-names-and-labels
        // Although that isn't explicitly said anywhere besides what's in the code.
        // The closest mention is in section about Parser Expression https://grafana.com/docs/loki/latest/logql/
        //
        // [a-zA-Z_][a-zA-Z0-9_]*
        let label_trim = label.get_ref().trim();
        let mut label_chars = label_trim.chars();
        if let Some(ch) = label_chars.next() {
            (ch.is_ascii_alphabetic() || ch == '_')
                && label_chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProxyConfig;
    use crate::event::Event;
    use crate::sinks::util::http::HttpSink;
    use crate::sinks::util::test::{build_test_server, load_sink};
    use crate::test_util;
    use futures::StreamExt;
    use std::convert::TryInto;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<LokiConfig>();
    }

    #[test]
    fn interpolate_labels() {
        let (config, _cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {label1 = "{{ foo }}", label2 = "some-static-label", label3 = "{{ foo }}", "{{ foo }}" = "{{ foo }}"}
            encoding = "json"
            remove_label_fields = true
        "#,
        )
        .unwrap();
        let sink = LokiSink::new(config);

        let mut e1 = Event::from("hello world");

        e1.as_mut_log().insert("foo", "bar");

        let mut record = sink.encode_event(e1).unwrap().into_parts().0;

        // HashMap -> Vec doesn't like keeping ordering
        record.labels.sort();

        // The final event should have timestamps and labels removed
        let expected_line = serde_json::to_string(&serde_json::json!({
            "message": "hello world",
        }))
        .unwrap();

        assert_eq!(record.event.event, expected_line);

        assert_eq!(record.labels[0], ("bar".to_string(), "bar".to_string()));
        assert_eq!(record.labels[1], ("label1".to_string(), "bar".to_string()));
        assert_eq!(
            record.labels[2],
            ("label2".to_string(), "some-static-label".to_string())
        );
        // make sure we can reuse fields across labels.
        assert_eq!(record.labels[3], ("label3".to_string(), "bar".to_string()));
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
        let sink = LokiSink::new(config);

        let mut e1 = Event::from("hello world");

        e1.as_mut_log().insert("foo", "bar");

        let record = sink.encode_event(e1).unwrap().into_parts().0;

        let expected_line = serde_json::to_string(&serde_json::json!({
            "message": "hello world",
        }))
        .unwrap();

        assert_eq!(record.event.event, expected_line);

        assert_eq!(record.labels[0], ("bar".to_string(), "bar".to_string()));
    }

    #[tokio::test]
    async fn healthcheck_includes_auth() {
        let (mut config, _cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "placeholder"}
            encoding = "json"
			auth.strategy = "basic"
			auth.user = "username"
			auth.password = "some_password"
        "#,
        )
        .unwrap();

        let addr = test_util::next_addr();
        let endpoint = format!("http://{}", addr);
        config.endpoint = endpoint
            .clone()
            .parse::<http::Uri>()
            .expect("could not create URI")
            .into();

        let (rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let tls = TlsSettings::from_options(&config.tls).expect("could not create TLS settings");
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(tls, &proxy).expect("could not create HTTP client");

        healthcheck(config.clone(), client)
            .await
            .expect("healthcheck failed");

        let output = rx.take(1).collect::<Vec<_>>().await;
        assert_eq!(
            Some(&http::header::HeaderValue::from_static(
                "Basic dXNlcm5hbWU6c29tZV9wYXNzd29yZA=="
            )),
            output[0].0.headers.get("authorization")
        );
    }

    #[tokio::test]
    async fn healthcheck_grafana_cloud() {
        test_util::trace_init();
        let (config, _cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://logs-prod-us-central1.grafana.net"
            encoding = "json"
            labels = {test_name = "placeholder"}
        "#,
        )
        .unwrap();

        let tls = TlsSettings::from_options(&config.tls).expect("could not create TLS settings");
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(tls, &proxy).expect("could not create HTTP client");

        healthcheck(config, client)
            .await
            .expect("healthcheck failed");
    }

    #[test]
    fn valid_label_names() {
        assert!(valid_label_name(&"name".try_into().unwrap()));
        assert!(valid_label_name(&" name ".try_into().unwrap()));
        assert!(valid_label_name(&"bee_bop".try_into().unwrap()));
        assert!(valid_label_name(&"a09b".try_into().unwrap()));

        assert!(!valid_label_name(&"0ab".try_into().unwrap()));
        assert!(!valid_label_name(&"*".try_into().unwrap()));
        assert!(!valid_label_name(&"".try_into().unwrap()));
        assert!(!valid_label_name(&" ".try_into().unwrap()));

        assert!(valid_label_name(&"{{field}}".try_into().unwrap()));
    }
}

#[cfg(feature = "loki-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        config::SinkConfig,
        sinks::util::test::load_sink,
        sinks::VectorSink,
        template::Template,
        test_util::{components, components::HTTP_SINK_TAGS, random_lines},
    };
    use bytes::Bytes;
    use chrono::{DateTime, Duration, Utc};
    use std::convert::TryFrom;
    use std::sync::Arc;
    use vector_core::event::{BatchNotifier, BatchStatus, Event, LogEvent};

    async fn build_sink(encoding: &str) -> (uuid::Uuid, VectorSink) {
        let stream = uuid::Uuid::new_v4();

        let config = format!(
            r#"
            endpoint = "http://localhost:3100"
            labels = {{test_name = "placeholder"}}
            encoding = "{}"
            remove_timestamp = false
            tenant_id = "default"
        "#,
            encoding
        );

        let (mut config, cx) = load_sink::<LokiConfig>(&config).unwrap();

        let test_name = config
            .labels
            .get_mut(&Template::try_from("test_name").unwrap())
            .unwrap();
        assert_eq!(test_name.get_ref(), &Bytes::from("placeholder"));

        *test_name = Template::try_from(stream.to_string()).unwrap();

        let (sink, _) = config.build(cx).await.unwrap();

        (stream, sink)
    }

    fn add_batch_notifier(events: &[Event], batch: Arc<BatchNotifier>) -> Vec<Event> {
        events
            .iter()
            .map(|event| event.clone().into_log().with_batch_notifier(&batch).into())
            .collect()
    }

    #[tokio::test]
    async fn text() {
        let (stream, sink) = build_sink("text").await;

        let lines = random_lines(100).take(10).collect::<Vec<_>>();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let events = lines
            .clone()
            .into_iter()
            .map(move |line| Event::from(LogEvent::from(line).with_batch_notifier(&batch)));
        components::sink_send_all(sink, events, &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
        assert_eq!(lines.len(), outputs.len());
        for (i, output) in outputs.iter().enumerate() {
            assert_eq!(output, &lines[i]);
        }
    }

    #[tokio::test]
    async fn json() {
        let (stream, sink) = build_sink("json").await;

        let events = random_lines(100)
            .take(10)
            .map(Event::from)
            .collect::<Vec<_>>();
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        components::sink_send_all(sink, add_batch_notifier(&events, batch), &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
        assert_eq!(events.len(), outputs.len());
        for (i, output) in outputs.iter().enumerate() {
            let expected_json = serde_json::to_string(&events[i].as_log()).unwrap();
            assert_eq!(output, &expected_json);
        }
    }

    // https://github.com/timberio/vector/issues/7815
    #[tokio::test]
    async fn json_nested_fields() {
        let (stream, sink) = build_sink("json").await;

        let events = random_lines(100)
            .take(10)
            .map(|line| {
                let mut event = Event::from(line);
                let log = event.as_mut_log();
                log.insert("foo.bar", "baz");
                event
            })
            .collect::<Vec<_>>();
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        components::sink_send_all(sink, add_batch_notifier(&events, batch), &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
        assert_eq!(events.len(), outputs.len());
        for (i, output) in outputs.iter().enumerate() {
            let expected_json = serde_json::to_string(&events[i].as_log()).unwrap();
            assert_eq!(output, &expected_json);
        }
    }

    #[tokio::test]
    async fn logfmt() {
        let (stream, sink) = build_sink("logfmt").await;

        let events = random_lines(100)
            .take(10)
            .map(Event::from)
            .collect::<Vec<_>>();
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        components::sink_send_all(sink, add_batch_notifier(&events, batch), &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
        assert_eq!(events.len(), outputs.len());
        for (i, output) in outputs.iter().enumerate() {
            let expected_logfmt =
                encode_logfmt::to_string(events[i].clone().into_log().into_parts().0).unwrap();
            assert_eq!(output, &expected_logfmt);
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
            tenant_id = "default"
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

        components::sink_send_all(sink, events, &HTTP_SINK_TAGS).await;

        let (_, outputs1) = fetch_stream(stream1.to_string(), "default").await;
        let (_, outputs2) = fetch_stream(stream2.to_string(), "default").await;

        assert_eq!(outputs1.len() + outputs2.len(), lines.len());

        for (i, output) in outputs1.iter().enumerate() {
            let index = (i % 5) * 2;
            assert_eq!(output, &lines[index]);
        }

        for (i, output) in outputs2.iter().enumerate() {
            let index = ((i % 5) * 2) + 1;
            assert_eq!(output, &lines[index]);
        }
    }

    #[tokio::test]
    async fn interpolate_stream_key() {
        let stream = uuid::Uuid::new_v4();

        let (mut config, cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {"{{ stream_key }}" = "placeholder"}
            encoding = "text"
            tenant_id = "default"
        "#,
        )
        .unwrap();
        config.labels.insert(
            Template::try_from("{{ stream_key }}").unwrap(),
            Template::try_from(stream.to_string()).unwrap(),
        );

        let (sink, _) = config.build(cx).await.unwrap();

        let lines = random_lines(100).take(10).collect::<Vec<_>>();

        let mut events = lines
            .clone()
            .into_iter()
            .map(Event::from)
            .collect::<Vec<_>>();

        for i in 0..10 {
            let event = events.get_mut(i).unwrap();
            event.as_mut_log().insert("stream_key", "test_name");
        }

        components::sink_send_all(sink, events, &HTTP_SINK_TAGS).await;

        let (_, outputs) = fetch_stream(stream.to_string(), "default").await;

        assert_eq!(outputs.len(), lines.len());

        for (i, output) in outputs.iter().enumerate() {
            assert_eq!(output, &lines[i]);
        }
    }

    #[tokio::test]
    async fn many_tenants() {
        let stream = uuid::Uuid::new_v4();

        let (mut config, cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "placeholder"}
            encoding = "text"
            tenant_id = "{{ tenant_id }}"
        "#,
        )
        .unwrap();

        let test_name = config
            .labels
            .get_mut(&Template::try_from("test_name").unwrap())
            .unwrap();
        assert_eq!(test_name.get_ref(), &Bytes::from("placeholder"));

        *test_name = Template::try_from(stream.to_string()).unwrap();

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
                event.as_mut_log().insert("tenant_id", "tenant1");
            } else {
                event.as_mut_log().insert("tenant_id", "tenant2");
            }
        }

        components::sink_send_all(sink, events, &HTTP_SINK_TAGS).await;

        let (_, outputs1) = fetch_stream(stream.to_string(), "tenant1").await;
        let (_, outputs2) = fetch_stream(stream.to_string(), "tenant2").await;

        assert_eq!(outputs1.len() + outputs2.len(), lines.len());

        for (i, output) in outputs1.iter().enumerate() {
            let index = (i % 5) * 2;
            assert_eq!(output, &lines[index]);
        }

        for (i, output) in outputs2.iter().enumerate() {
            let index = ((i % 5) * 2) + 1;
            assert_eq!(output, &lines[index]);
        }
    }

    #[tokio::test]
    async fn out_of_order_drop() {
        let batch_size = 5;
        let lines = random_lines(100).take(10).collect::<Vec<_>>();
        let mut events = lines
            .clone()
            .into_iter()
            .map(Event::from)
            .collect::<Vec<_>>();

        let base = chrono::Utc::now() - Duration::seconds(20);
        for (i, event) in events.iter_mut().enumerate() {
            let log = event.as_mut_log();
            log.insert(
                log_schema().timestamp_key(),
                base + Duration::seconds(i as i64),
            );
        }
        // first event of the second batch is out-of-order.
        events[batch_size]
            .as_mut_log()
            .insert(log_schema().timestamp_key(), base);

        let mut expected = events.clone();
        expected.remove(batch_size);

        test_out_of_order_events(OutOfOrderAction::Drop, batch_size, events, expected).await;
    }

    #[tokio::test]
    async fn out_of_order_rewrite() {
        let batch_size = 5;
        let lines = random_lines(100).take(10).collect::<Vec<_>>();
        let mut events = lines
            .clone()
            .into_iter()
            .map(Event::from)
            .collect::<Vec<_>>();

        let base = chrono::Utc::now() - Duration::seconds(20);
        for (i, event) in events.iter_mut().enumerate() {
            let log = event.as_mut_log();
            log.insert(
                log_schema().timestamp_key(),
                base + Duration::seconds(i as i64),
            );
        }
        // first event of the second batch is out-of-order.
        events[batch_size]
            .as_mut_log()
            .insert(log_schema().timestamp_key(), base);

        let mut expected = events.clone();
        let time = get_timestamp(&expected[batch_size - 1]);
        // timestamp is rewriten with latest timestamp of the first batch
        expected[batch_size]
            .as_mut_log()
            .insert(log_schema().timestamp_key(), time);

        test_out_of_order_events(
            OutOfOrderAction::RewriteTimestamp,
            batch_size,
            events,
            expected,
        )
        .await;
    }

    #[tokio::test]
    async fn out_of_order_per_partition() {
        let batch_size = 2;
        let big_lines = random_lines(1_000_000).take(2);
        let small_lines = random_lines(1).take(20);
        let mut events = big_lines
            .into_iter()
            .chain(small_lines)
            .map(Event::from)
            .collect::<Vec<_>>();

        let base = chrono::Utc::now() - Duration::seconds(30);
        for (i, event) in events.iter_mut().enumerate() {
            let log = event.as_mut_log();
            log.insert(
                log_schema().timestamp_key(),
                base + Duration::seconds(i as i64),
            );
        }

        // So, since all of the events are of the same partition, and if there is concurrency,
        // then if ordering inside paritions isn't upheld, the big line events will take longer
        // time to flush than small line events so loki will receive smaller ones before large
        // ones hence out of order events.
        test_out_of_order_events(OutOfOrderAction::Drop, batch_size, events.clone(), events).await;
    }

    async fn test_out_of_order_events(
        action: OutOfOrderAction,
        batch_size: usize,
        events: Vec<Event>,
        expected: Vec<Event>,
    ) {
        crate::test_util::trace_init();
        let stream = uuid::Uuid::new_v4();

        let (mut config, cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "placeholder"}
            encoding = "text"
            tenant_id = "default"
        "#,
        )
        .unwrap();
        config.out_of_order_action = action;
        config.labels.insert(
            Template::try_from("test_name").unwrap(),
            Template::try_from(stream.to_string()).unwrap(),
        );
        config.batch.max_events = Some(batch_size);
        config.batch.max_bytes = Some(4_000_000);

        let (sink, _) = config.build(cx).await.unwrap();
        components::sink_send_all(sink, events.clone(), &HTTP_SINK_TAGS).await;

        let (timestamps, outputs) = fetch_stream(stream.to_string(), "default").await;
        assert_eq!(expected.len(), outputs.len());
        assert_eq!(expected.len(), timestamps.len());
        for (i, output) in outputs.iter().enumerate() {
            assert_eq!(
                &expected[i]
                    .as_log()
                    .get(log_schema().message_key())
                    .unwrap()
                    .to_string_lossy(),
                output,
            )
        }
        for (i, ts) in timestamps.iter().enumerate() {
            assert_eq!(get_timestamp(&expected[i]).timestamp_nanos(), *ts);
        }
    }

    fn get_timestamp(event: &Event) -> DateTime<Utc> {
        *event
            .as_log()
            .get(log_schema().timestamp_key())
            .unwrap()
            .as_timestamp()
            .unwrap()
    }

    async fn fetch_stream(stream: String, tenant: &str) -> (Vec<i64>, Vec<String>) {
        let query = format!("%7Btest_name%3D\"{}\"%7D", stream);
        let query = format!(
            "http://localhost:3100/loki/api/v1/query_range?query={}&direction=forward",
            query
        );

        let res = reqwest::Client::new()
            .get(&query)
            .header("X-Scope-OrgID", tenant)
            .send()
            .await
            .unwrap();

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

        // the array looks like: [ts, line].
        let timestamps = values
            .iter()
            .map(|v| v[0].as_str().unwrap().parse().unwrap())
            .collect();
        let lines = values
            .iter()
            .map(|v| v[1].as_str().unwrap().to_string())
            .collect();
        (timestamps, lines)
    }
}
