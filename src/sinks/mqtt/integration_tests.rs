use crate::config::{SinkConfig, SinkContext};
use crate::sinks::mqtt::config::MqttQoS;
use crate::sinks::mqtt::MqttSinkConfig;
use crate::template::Template;
use crate::test_util::components::{run_and_assert_sink_compliance, SINK_TAGS};
use crate::test_util::{random_lines_with_stream, trace_init};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::time::Duration;

fn mqtt_broker_address() -> String {
    let result = std::env::var("MQTT_BROKER_ADDRESS").unwrap_or_else(|_| "emqx".into());
    result
}

fn mqtt_broker_port() -> u16 {
    let result = std::env::var("MQTT_BROKER_PORT")
        .unwrap_or_else(|_| "1883".into())
        .parse::<u16>()
        .expect("Cannot parse as u16");
    result
}

#[tokio::test]
async fn mqtt_happy() {
    trace_init();

    let topic = "test";
    let cnf = MqttSinkConfig {
        host: mqtt_broker_address(),
        port: mqtt_broker_port(),
        topic: Template::try_from(topic).expect("Cannot parse the topic template"),
        quality_of_service: MqttQoS::AtLeastOnce,
        ..Default::default()
    };

    let cx = SinkContext::new_test();
    let (sink, healthcheck) = cnf.build(cx).await.expect("Cannot build the sink");
    healthcheck.await.expect("Health check failed");

    // prepare consumer
    let mut mqttoptions = MqttOptions::new(
        "integration-test-consumer",
        mqtt_broker_address(),
        mqtt_broker_port(),
    );
    mqttoptions.set_keep_alive(Duration::from_secs(5));

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
    client
        .subscribe("test", QoS::AtLeastOnce)
        .await
        .expect("Cannot subscribe to the topic");

    let num_events = 10;
    let (input, events) = random_lines_with_stream(100, num_events, None);
    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

    // loop instead of iter so we can set a timeout
    let mut failures = 0;
    let mut message_count = 0;
    while failures < 5 && message_count < input.len() {
        if let Ok(try_msg) = tokio::time::timeout(Duration::from_secs(1), eventloop.poll()).await {
            let _msg = try_msg.expect("Cannot extract the message");
            message_count += 1;
        } else {
            failures += 1;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    assert_eq!(message_count, input.len());
}
