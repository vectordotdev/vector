use futures::{future::ready, stream};
use serde::Deserialize;
use vector_core::event::{Metric, MetricKind, MetricValue};

use super::{config::StackdriverConfig, *};
use crate::{
    config::SinkContext,
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<StackdriverConfig>();
}

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let config = StackdriverConfig::generate_config().to_string();
    let mut config = StackdriverConfig::deserialize(toml::de::ValueDeserializer::new(&config))
        .expect("config should be valid");

    // If we don't override the credentials path/API key, it tries to directly call out to the Google Instance
    // Metadata API, which we clearly don't have in unit tests. :)
    config.auth.credentials_path = None;
    config.auth.api_key = Some("fake".to_string().into());
    config.endpoint = mock_endpoint.to_string();

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let event = Event::Metric(Metric::new(
        "gauge-test",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 1_f64 },
    ));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
}
