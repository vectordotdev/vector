use std::{
    collections::HashMap,
    net::UdpSocket,
    time::{Duration, SystemTime},
};

use chrono::Utc;
use indoc::indoc;
use tokio::{io::AsyncWriteExt, net::TcpStream};

use super::{DatadogAgentConfig, LOGS, METRICS};
use crate::{
    config::{GenerateConfig, SourceConfig, SourceContext},
    event::{EventStatus, Metric, MetricValue, Value},
    schema,
    test_util::{next_addr, spawn_collect_n, spawn_collect_ready, wait_for_tcp},
    SourceSender,
};

fn agent_address() -> String {
    std::env::var("AGENT_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8181".to_owned())
}

fn trace_agent_url() -> String {
    std::env::var("TRACE_AGENT_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8126/v0.4/traces".to_owned())
}

fn agent_health_address() -> String {
    std::env::var("AGENT_HEALTH_ADDRESS").unwrap_or_else(|_| "http://0.0.0.0:8182".to_owned())
}

fn metrics_v1_agent_health_address() -> String {
    std::env::var("METRICS_V1_AGENT_HEALTH_ADDRESS")
        .unwrap_or_else(|_| "http://0.0.0.0:8184".to_owned())
}

fn metrics_v2_agent_health_address() -> String {
    std::env::var("METRICS_V2_AGENT_HEALTH_ADDRESS")
        .unwrap_or_else(|_| "http://0.0.0.0:8185".to_owned())
}

fn metrics_v1_dsd_address() -> String {
    std::env::var("METRICS_V1_DSD_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8125".to_owned())
}

fn metrics_v2_dsd_address() -> String {
    std::env::var("METRICS_V2_DSD_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8126".to_owned())
}

fn trace_agent_health_address() -> String {
    std::env::var("TRACE_AGENT_HEALTH_ADDRESS").unwrap_or_else(|_| "http://0.0.0.0:8183".to_owned())
}

const AGENT_TIMEOUT: u64 = 60; // timeout in seconds

async fn wait_for_healthy(address: String) {
    let start = SystemTime::now();
    while start
        .elapsed()
        .map(|value| value.as_secs() < AGENT_TIMEOUT)
        .unwrap_or(false)
    {
        if reqwest::get(&address)
            .await
            .map(|res| res.status().is_success())
            .unwrap_or(false)
        {
            return;
        }
        // wait a second before retry...
        tokio::time::sleep(Duration::new(1, 0)).await;
    }
    panic!("Unable to reach the Datadog Agent. Check that it's started and that the health endpoint is available at {}.", address);
}

async fn wait_for_healthy_agent() {
    wait_for_healthy(agent_health_address()).await
}

async fn wait_for_healthy_metrics_v1_agent() {
    wait_for_healthy(metrics_v1_agent_health_address()).await
}

async fn wait_for_healthy_metrics_v2_agent() {
    wait_for_healthy(metrics_v2_agent_health_address()).await
}

async fn wait_for_healthy_trace_agent() {
    wait_for_healthy(trace_agent_health_address()).await
}

#[tokio::test]
async fn wait_for_message() {
    wait_for_healthy_agent().await;

    let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let schema_definitions = HashMap::from([
        (Some(LOGS.to_owned()), schema::Definition::empty()),
        (Some(METRICS.to_owned()), schema::Definition::empty()),
    ]);
    let context = SourceContext::new_test(sender, Some(schema_definitions));
    tokio::spawn(async move {
        let config: DatadogAgentConfig = DatadogAgentConfig::generate_config().try_into().unwrap();
        config.build(context).await.unwrap().await.unwrap()
    });
    let events = spawn_collect_n(
        async move {
            let agent_logs_address = agent_address();
            wait_for_tcp(agent_logs_address.clone()).await;

            let mut stream = TcpStream::connect(&agent_logs_address).await.unwrap();
            let data = "hello world\nit's vector speaking\n";
            stream.write_all(data.as_bytes()).await.unwrap();
        },
        recv,
        2,
    )
    .await;
    assert_eq!(events.len(), 2);
    let event = events.get(0).unwrap().as_log();
    let msg = event.get("message").unwrap().coerce_to_bytes();
    assert_eq!(msg, "hello world");
    let event = events.get(1).unwrap().as_log();
    let msg = event.get("message").unwrap().coerce_to_bytes();
    assert_eq!(msg, "it's vector speaking");
}

#[tokio::test]
async fn wait_for_traces() {
    wait_for_healthy_trace_agent().await;

    let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let schema_definitions = HashMap::from([
        (Some(LOGS.to_owned()), schema::Definition::empty()),
        (Some(METRICS.to_owned()), schema::Definition::empty()),
    ]);
    let context = SourceContext::new_test(sender, Some(schema_definitions));
    tokio::spawn(async move {
        let config_raw = "address = \"0.0.0.0:8081\"".to_string();
        let config = toml::from_str::<DatadogAgentConfig>(config_raw.as_str()).unwrap();
        config.build(context).await.unwrap().await.unwrap()
    });
    let events = spawn_collect_n(
        async move {
            let url = trace_agent_url();
            let body = get_simple_trace();
            let client = reqwest::Client::new();
            let res = client
                .get(&url)
                .body(body)
                .header("Content-Type", "application/json")
                .send()
                .await;

            assert!(res.unwrap().status().is_success());
        },
        recv,
        1,
    )
    .await;

    assert_eq!(events.len(), 1);
    let trace = events.get(0).unwrap().as_trace();
    let spans = trace.get("spans").unwrap().as_array().unwrap();
    assert_eq!(spans.len(), 1);
    let span = spans.get(0).unwrap();
    assert_eq!(span.get("name"), Some(&Value::from("a_name")));
    assert_eq!(span.get("service"), Some(&Value::from("a_service")));
    assert_eq!(span.get("resource"), Some(&Value::from("a_resource")));
    assert_eq!(span.get("name"), Some(&Value::from("a_name")));
    assert_eq!(span.get("trace_id"), Some(&Value::Integer(123)));
    assert_eq!(span.get("span_id"), Some(&Value::Integer(456)));
    assert_eq!(span.get("parent_id"), Some(&Value::Integer(789)));
}

fn get_simple_trace() -> String {
    format!(
        indoc! {r#"
        [
            [
                {{
                "service": "a_service",
                "name": "a_name",
                "resource": "a_resource",
                "trace_id": 123,
                "span_id": 456,
                "parent_id": 789,
                "start": {},
                "duration": 10,
                "error": 404,
                "meta": {{
                    "foo": "bar"
                }},
                "metrics": {{
                    "foobar": 123
                }},
                "type": "a type"
                }}
            ]
        ]
        "#},
        Utc::now().timestamp_nanos()
    )
}

#[tokio::test]
async fn wait_for_metrics_v1() {
    wait_for_healthy_metrics_v1_agent().await;
    wait_for_metrics(8082, metrics_v1_dsd_address()).await
}

#[tokio::test]
async fn wait_for_metrics_v2() {
    wait_for_healthy_metrics_v2_agent().await;
    wait_for_metrics(8083, metrics_v2_dsd_address()).await
}

async fn wait_for_metrics(vector_port: u16, dsd_address: String) {
    let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let schema_definitions = HashMap::from([
        (Some(LOGS.to_owned()), schema::Definition::empty()),
        (Some(METRICS.to_owned()), schema::Definition::empty()),
    ]);
    let context = SourceContext::new_test(sender, Some(schema_definitions));
    tokio::spawn(async move {
        let config_raw = format!("address = \"0.0.0.0:{}\"", vector_port);
        let config = toml::from_str::<DatadogAgentConfig>(config_raw.as_str()).unwrap();
        config.build(context).await.unwrap().await.unwrap()
    });

    let events = spawn_collect_ready(
        async move {
            // Earlier wait_for_healthy_agent() should be enough to have a working agent
            let bind = next_addr();
            let socket = UdpSocket::bind(bind)
                .map_err(|error| panic!("{:}", error))
                .ok()
                .unwrap();
            let statsd_metrics = (indoc! { r#"
                    custom_gauge_test:60|g|#vector-intg-test,tag:value
                    custom_count_test:42|c|#vector-intg-test,foo:bar
                "# })
            .to_string();
            assert_eq!(
                socket
                    .send_to(statsd_metrics.as_bytes(), dsd_address)
                    .map_err(|error| panic!("{:}", error))
                    .ok()
                    .unwrap(),
                statsd_metrics.as_bytes().len()
            );
        },
        recv,
        // We wait 30 seconds to let agent enough time to notice there is a valid endpoint
        // for metrics and flush pending metrics (the agent config has been tuned for
        // fast retries).
        30,
    )
    .await;

    // clean up everything that was not
    let mut filtered_metrics = events
        .into_iter()
        .filter_map(|m| m.try_into_metric())
        .filter(|m| m.name() == "custom_gauge_test" || m.name() == "custom_count_test")
        .collect::<Vec<Metric>>();

    filtered_metrics.sort_by(|m1, m2| m1.name().cmp(m2.name()));

    // Strictly two elements should remain
    assert_eq!(filtered_metrics.len(), 2);

    let metric = filtered_metrics.get(0).unwrap();
    assert_eq!(metric.name(), "custom_count_test");
    assert_eq!(metric.value(), &MetricValue::Counter { value: 42.0 });
    assert_eq!(metric.tags().unwrap().get("vector-intg-test").unwrap(), "");
    assert_eq!(metric.tags().unwrap().get("foo").unwrap(), "bar");

    let metric = filtered_metrics.get(1).unwrap();
    assert_eq!(metric.name(), "custom_gauge_test");
    assert_eq!(metric.value(), &MetricValue::Gauge { value: 60.0 });
    assert_eq!(metric.tags().unwrap().get("vector-intg-test").unwrap(), "");
    assert_eq!(metric.tags().unwrap().get("tag").unwrap(), "value");
}
