use indoc::indoc;
use vector_core::event::{BatchNotifier, BatchStatus};

use crate::{
    config::SinkConfig, sinks::datadog::logs::DatadogLogsConfig, sinks::util::test::load_sink,
    test_util::generate_lines_with_stream,
};

#[tokio::test]
async fn to_real_v2_endpoint() {
    let config = indoc! {r#"
        default_api_key = "atoken"
        compression = "none"
    "#};
    let api_key = std::env::var("CI_TEST_DATADOG_API_KEY")
        .expect("couldn't find the Datatog api key in environment variables");
    let config = config.replace("atoken", &api_key);
    let (config, cx) = load_sink::<DatadogLogsConfig>(config.as_str()).unwrap();

    let (sink, _) = config.build(cx).await.unwrap();
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let generator = |index| format!("this is a log with index {}", index);
    let (_, events) = generate_lines_with_stream(generator, 10, Some(batch));

    let _ = sink.run(events).await.unwrap();
    assert_eq!(receiver.await, BatchStatus::Delivered);
}
