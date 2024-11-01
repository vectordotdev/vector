use indoc::indoc;
use vector_lib::event::{BatchNotifier, BatchStatus};

use super::config::DatadogLogsConfig;
use crate::{
    config::SinkConfig,
    sinks::util::test::load_sink,
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        generate_lines_with_stream,
    },
};

#[tokio::test]
async fn to_real_v2_endpoint() {
    let config = indoc! {r#"
        default_api_key = "atoken"
        compression = "none"
    "#};
    let api_key = std::env::var("TEST_DATADOG_API_KEY")
        .expect("couldn't find the Datadog api key in environment variables");
    assert!(!api_key.is_empty(), "$TEST_DATADOG_API_KEY required");
    let config = config.replace("atoken", &api_key);
    let (config, cx) = load_sink::<DatadogLogsConfig>(config.as_str()).unwrap();

    let (sink, _) = config.build(cx).await.unwrap();
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let generator = |index| format!("this is a log with index {}", index);
    let (_, events) = generate_lines_with_stream(generator, 10, Some(batch));

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

    assert_eq!(receiver.await, BatchStatus::Delivered);
}
