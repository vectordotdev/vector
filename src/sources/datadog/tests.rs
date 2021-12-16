#![cfg(feature = "datadog-agent-integration-tests")]

use super::agent::DatadogAgentConfig;
use crate::config::{GenerateConfig, SourceConfig, SourceContext};
use crate::event::EventStatus;
use crate::test_util::spawn_collect_n;
use crate::Pipeline;
use std::io::prelude::*;
use std::net::TcpStream;

#[tokio::test]
async fn wait_for_message() {
    let (sender, recv) = Pipeline::new_test_finalize(EventStatus::Delivered);
    let context = SourceContext::new_test(sender);
    tokio::spawn(async move {
        let config: DatadogAgentConfig = DatadogAgentConfig::generate_config().try_into().unwrap();
        config.build(context).await.unwrap().await.unwrap()
    });
    let events = spawn_collect_n(
        async move {
            let mut stream = TcpStream::connect("0.0.0.0:8181").unwrap();
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
