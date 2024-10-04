use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use chrono::Utc;
use indoc::indoc;
use tokio::{io::AsyncWriteExt, net::TcpStream};

use super::{DatadogAgentConfig, LOGS, METRICS};
use crate::{
    config::{GenerateConfig, SourceConfig, SourceContext},
    event::{EventStatus, Value},
    schema,
    test_util::{spawn_collect_n, wait_for_tcp},
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

async fn wait_for_healthy_trace_agent() {
    wait_for_healthy(trace_agent_health_address()).await
}

#[tokio::test]
async fn wait_for_message() {
    wait_for_healthy_agent().await;

    let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let schema_definitions = HashMap::from([
        (
            Some(LOGS.to_owned()),
            schema::Definition::empty_legacy_namespace(),
        ),
        (
            Some(METRICS.to_owned()),
            schema::Definition::empty_legacy_namespace(),
        ),
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
    let event = events.first().unwrap().as_log();
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
        (
            Some(LOGS.to_owned()),
            schema::Definition::empty_legacy_namespace(),
        ),
        (
            Some(METRICS.to_owned()),
            schema::Definition::empty_legacy_namespace(),
        ),
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
    let trace = events.first().unwrap().as_trace();
    let spans = trace.get("spans").unwrap().as_array().unwrap();
    assert_eq!(spans.len(), 1);
    let span = spans.first().unwrap();
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
        Utc::now()
            .timestamp_nanos_opt()
            .expect("Timestamp out of range")
    )
}
