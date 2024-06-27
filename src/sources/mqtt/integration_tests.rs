#![cfg(feature = "mqtt-integration-tests")]
#![cfg(test)]

use crate::test_util::trace_init;
use crate::test_util::{components::SOURCE_TAGS, random_lines_with_stream, random_string};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::{collections::HashSet, time::Duration};

use futures::StreamExt;
use tokio::time::timeout;

use super::MqttSourceConfig;
use crate::{
    config::{log_schema, SourceConfig, SourceContext},
    event::Event,
    test_util::components::assert_source_compliance,
    SourceSender,
};

fn mqtt_broker_address() -> String {
    std::env::var("MQTT_BROKER_ADDRESS").unwrap_or_else(|_| "emqx".into())
}

fn mqtt_broker_port() -> u16 {
    std::env::var("MQTT_BROKER_PORT")
        .unwrap_or_else(|_| "1883".into())
        .parse::<u16>()
        .expect("Cannot parse as u16")
}

async fn send_test_events(client: &AsyncClient, topic: &str, messages: &Vec<String>) {
    for message in messages {
        client
            .publish(topic, QoS::AtLeastOnce, false, message.as_bytes())
            .await
            .unwrap();
    }
}

async fn get_mqtt_client() -> AsyncClient {
    let mut mqtt_options = MqttOptions::new(
        "integration-test-producer",
        mqtt_broker_address(),
        mqtt_broker_port(),
    );
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

    tokio::spawn(async move {
        loop {
            eventloop.poll().await.unwrap();
        }
    });

    client
}

#[tokio::test]
async fn mqtt_happy() {
    trace_init();
    let topic = "source-test";
    // We always want new client ID. If it were stable, subsequent tests could receive data sent in previous runs.
    let client_id = format!("sourceTest{}", random_string(6));
    let num_events = 10;
    let (input, _events) = random_lines_with_stream(100, num_events, None);

    assert_source_compliance(&SOURCE_TAGS, async {
        let config = MqttSourceConfig {
            host: mqtt_broker_address(),
            port: mqtt_broker_port(),
            client_id: Some(client_id),
            topic: topic.to_owned(),
            ..MqttSourceConfig::default()
        };

        let (tx, rx) = SourceSender::new_test();
        tokio::spawn(async move {
            config
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap()
                .await
                .unwrap()
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = get_mqtt_client().await;
        send_test_events(&client, topic, &input).await;

        let mut expected_messages: HashSet<_> = input.into_iter().collect();

        let events: Vec<Event> = timeout(Duration::from_secs(2), rx.take(num_events).collect())
            .await
            .unwrap();

        for event in events {
            let message = event
                .as_log()
                .get(log_schema().message_key_target_path().unwrap())
                .unwrap()
                .to_string_lossy();
            if !expected_messages.remove(message.as_ref()) {
                panic!("Received unexpected message: {:?}", message);
            }
        }
        assert!(expected_messages.is_empty());
    })
    .await;
}
