//! Unit tests for the `honeycomb` sink.

use futures::{future::ready, stream};
use serde::Deserialize;

use crate::{
    sinks::prelude::*,
    test_util::{
        components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

use super::config::HoneycombConfig;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<HoneycombConfig>();
}

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let config = HoneycombConfig::generate_config().to_string();
    let mut config = HoneycombConfig::deserialize(toml::de::ValueDeserializer::new(&config))
        .expect("config should be valid");
    config.endpoint = mock_endpoint.to_string();

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let event = Event::Log(LogEvent::from("simple message"));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
}
