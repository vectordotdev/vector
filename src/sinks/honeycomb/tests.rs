//! Unit tests for the `honeycomb` sink.

use futures::{future::ready, stream};

use super::config::HoneycombConfig;
use crate::{
    sinks::{prelude::*, util::HttpEndpoint},
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<HoneycombConfig>();
}

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let mut config: HoneycombConfig =
        serde_json::from_value(HoneycombConfig::generate_config()).expect("config should be valid");
    config.endpoint = HttpEndpoint::parse(&mock_endpoint.to_string()).unwrap();

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let event = Event::Log(LogEvent::from("simple message"));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
}
