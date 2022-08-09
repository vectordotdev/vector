#![cfg(feature = "apex-integration-tests")]
#![cfg(test)]

use vector_core::event::{BatchNotifier, BatchStatus, Event, LogEvent};

use super::ApexSinkConfig;
use crate::{
    config::SinkConfig,
    sinks::util::test::load_sink,
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        generate_events_with_stream,
    },
};
use serde_json::json;

fn mock_apex_address() -> String {
    std::env::var("MOCK_APEX_ADDRESS").unwrap_or_else(|_| "http://localhost:4567".into())
}

fn mock_apex_api_token() -> String {
    std::env::var("MOCK_APEX_API_TOKEN").unwrap_or_else(|_| "token".into())
}

fn line_generator(index: usize) -> String {
    format!("random line {}", index)
}

fn event_generator(index: usize) -> Event {
    Event::Log(
        LogEvent::try_from(json!({
            "message": line_generator(index),
            "level": "info",
        }))
        .unwrap(),
    )
}

#[tokio::test]
async fn apex_test() {
    let config = format!(
        r#"
            uri = "{}"
            project_id = "integration-test"
            api_token = "{}"
        "#,
        mock_apex_address(),
        mock_apex_api_token(),
    );

    let (config, cx) = load_sink::<ApexSinkConfig>(&config).unwrap();
    let (sink, _) = config.build(cx).await.unwrap();

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (_lines, events) = generate_events_with_stream(event_generator, 10, Some(batch));

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
}
