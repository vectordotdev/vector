use futures::{future::ready, stream};
use vector_lib::{
    configurable::component::GenerateConfig,
    event::{Event, LogEvent},
};

use super::config::AppsignalConfig;
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::util::HttpEndpoint,
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let mut config: AppsignalConfig =
        serde_json::from_value(AppsignalConfig::generate_config()).expect("config should be valid");
    config.endpoint = HttpEndpoint::parse(&mock_endpoint.to_string()).unwrap();

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let event = Event::Log(LogEvent::from("simple message"));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
}
