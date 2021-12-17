use super::DatadogAgentConfig;
use crate::config::{GenerateConfig, SourceConfig, SourceContext};
use crate::event::EventStatus;
use crate::test_util::spawn_collect_n;
use crate::Pipeline;
use std::io::prelude::*;
use std::net::TcpStream;
use std::time::{Duration, SystemTime};

fn agent_address() -> String {
    std::env::var("AGENT_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8181".to_owned())
}

fn agent_health_address() -> String {
    std::env::var("AGENT_HEALTH_ADDRESS").unwrap_or_else(|_| "http://0.0.0.0:8182".to_owned())
}

const AGENT_TIMEOUT: u64 = 60; // timeout in seconds

async fn wait_for_agent() {
    let start = SystemTime::now();
    let address = agent_health_address();
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
    panic!("unable to reach the datadog agent, check that it's started");
}

#[tokio::test]
async fn wait_for_message() {
    wait_for_agent().await;
    let (sender, recv) = Pipeline::new_test_finalize(EventStatus::Delivered);
    let context = SourceContext::new_test(sender);
    tokio::spawn(async move {
        let config: DatadogAgentConfig = DatadogAgentConfig::generate_config().try_into().unwrap();
        config.build(context).await.unwrap().await.unwrap()
    });
    let events = spawn_collect_n(
        async move {
            let address = agent_address();
            let mut stream = TcpStream::connect(&address).unwrap();
            let data = "hello world\nit's vector speaking\n";
            stream.write_all(data.as_bytes()).unwrap();
        },
        recv,
        2,
    )
    .await;
    assert_eq!(events.len(), 2);
    let event = events.get(0).unwrap().as_log();
    let msg = event.get("message").unwrap().as_bytes();
    assert_eq!(msg, "hello world");
    let event = events.get(1).unwrap().as_log();
    let msg = event.get("message").unwrap().as_bytes();
    assert_eq!(msg, "it's vector speaking");
}
