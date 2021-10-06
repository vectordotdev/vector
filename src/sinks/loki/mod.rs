//! Loki sink
//!
//! This sink provides downstream support for `Loki` via
//! the v1 http json endpoint.
//!
//! <https://github.com/grafana/loki/tree/v1.6.1/docs>
//!
//! This sink uses `PartitionBatching` to partition events
//! by streams. There must be at least one valid set of labels.
//!
//! If an event produces no labels, this can happen if the template
//! does not match, we will add a default label `{agent="vector"}`.
mod config;
mod healthcheck;
mod service;
mod sink;

use crate::config::SinkDescription;
use config::LokiConfig;

pub use config::OutOfOrderAction;

inventory::submit! {
    SinkDescription::new::<LokiConfig>("loki")
}

#[cfg(test)]
mod tests {
    use super::config::LokiConfig;
    use super::healthcheck::healthcheck;
    use super::sink::LokiSink;
    use crate::config::ProxyConfig;
    use crate::event::Event;
    use crate::http::HttpClient;
    use crate::sinks::util::test::{build_test_server, load_sink};
    use crate::test_util;
    use crate::tls::TlsSettings;
    use futures::StreamExt;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<LokiConfig>();
    }

    #[test]
    fn interpolate_labels() {
        let (config, cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels = {label1 = "{{ foo }}", label2 = "some-static-label", label3 = "{{ foo }}", "{{ foo }}" = "{{ foo }}"}
            encoding = "json"
            remove_label_fields = true
        "#,
        )
        .unwrap();
        let client = config.build_client(cx.clone()).unwrap();
        let sink = LokiSink::new(config, client, cx).unwrap();

        let mut e1 = Event::from("hello world");

        e1.as_mut_log().insert("foo", "bar");

        let mut record = sink.encoder.encode_event(e1);

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
        let (config, cx) = load_sink::<LokiConfig>(
            r#"
            endpoint = "http://localhost:3100"
            labels.bar = "{{ foo }}"
            encoding.codec = "json"
            encoding.except_fields = ["foo"]
        "#,
        )
        .unwrap();
        let client = config.build_client(cx.clone()).unwrap();
        let sink = LokiSink::new(config, client, cx).unwrap();

        let mut e1 = Event::from("hello world");

        e1.as_mut_log().insert("foo", "bar");

        let record = sink.encoder.encode_event(e1);

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
}

#[cfg(feature = "loki-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::config::{valid_label_name, LokiConfig, OutOfOrderAction};
    use crate::config::log_schema;
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
