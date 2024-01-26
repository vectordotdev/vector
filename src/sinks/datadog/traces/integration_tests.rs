use futures::stream;
use indoc::indoc;

use crate::{
    config::SinkConfig,
    event::Event,
    sinks::{
        datadog::traces::{tests::simple_trace_event, DatadogTracesConfig},
        util::test::load_sink,
    },
    test_util::{
        components::{assert_sink_compliance, SINK_TAGS},
        map_event_batch_stream,
    },
};
use vector_lib::event::{BatchNotifier, BatchStatus};

#[tokio::test]
async fn to_real_traces_endpoint() {
    assert_sink_compliance(&SINK_TAGS, async {
        let config = indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
        "#};
        let api_key = std::env::var("TEST_DATADOG_API_KEY")
            .expect("couldn't find the Datadog api key in environment variables");
        assert!(!api_key.is_empty(), "TEST_DATADOG_API_KEY required");
        let config = config.replace("atoken", &api_key);
        let (config, cx) = load_sink::<DatadogTracesConfig>(config.as_str()).unwrap();

        let (sink, _) = config.build(cx).await.unwrap();
        let (batch, receiver) = BatchNotifier::new_with_receiver();

        let trace = vec![Event::Trace(
            simple_trace_event("a_trace".to_string()).with_batch_notifier(&batch),
        )];

        let stream = map_event_batch_stream(stream::iter(trace), Some(batch));

        sink.run(stream).await.unwrap();
        assert_eq!(receiver.await, BatchStatus::Delivered);
    })
    .await;
}
