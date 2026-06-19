#![cfg(feature = "mqtt-integration-tests")]
#![cfg(test)]

use std::{collections::HashSet, time::Duration};

use futures::StreamExt;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use tokio::time::timeout;

use crate::{
    SourceSender,
    common::mqtt::MqttCommonConfig,
    config::{SourceConfig, SourceContext, log_schema},
    event::{Event, EventStatus},
    serde::OneOrMany,
    sources::mqtt::MqttSourceConfig,
    test_util::{
        components::{SOURCE_TAGS, assert_source_compliance},
        random_lines_with_stream, random_string, trace_init,
    },
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

fn message_body(event: &Event) -> String {
    event
        .as_log()
        .get(log_schema().message_key_target_path().unwrap())
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

async fn get_mqtt_client() -> AsyncClient {
    // Unique client ID per producer: brokers that strictly enforce client-ID
    // uniqueness (e.g. RabbitMQ) otherwise kick a previous connection when tests
    // run concurrently, which manifests as spurious publish timeouts.
    let mut mqtt_options = MqttOptions::new(
        format!("integration-test-producer-{}", random_string(6)),
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
async fn mqtt_one_topic_happy() {
    trace_init();
    let topic = "source-test";
    // We always want new client ID. If it were stable, subsequent tests could receive data sent in previous runs.
    let client_id = format!("sourceTest{}", random_string(6));
    let num_events = 10;
    let (input, ..) = random_lines_with_stream(100, num_events, None);

    assert_source_compliance(&SOURCE_TAGS, async {
        let common = MqttCommonConfig {
            host: mqtt_broker_address(),
            port: mqtt_broker_port(),
            client_id: Some(client_id),
            ..Default::default()
        };

        let config = MqttSourceConfig {
            common,
            topic: OneOrMany::One(topic.to_owned()),
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
                panic!("Received unexpected message: {message:?}");
            }
        }
        assert!(expected_messages.is_empty());
    })
    .await;
}

/// With end-to-end acknowledgements enabled, a message that is received but not
/// successfully delivered (the sink rejects it) must not be acknowledged to the
/// broker, so the broker redelivers it. This proves the at-least-once guarantee
/// added by the `acknowledgements` option: no data is lost when a downstream
/// failure or crash occurs before the write is confirmed.
#[tokio::test]
async fn mqtt_redelivers_unacknowledged_messages() {
    trace_init();

    let topic = "source-redelivery-test";
    // A stable client ID so the second connection resumes the same persistent
    // session (the source sets `clean_session = false`); the broker then
    // redelivers any in-flight QoS 1 message that was never acknowledged.
    let client_id = format!("sourceAckTest{}", random_string(6));
    let message = random_string(32);

    let make_config = || MqttSourceConfig {
        common: MqttCommonConfig {
            host: mqtt_broker_address(),
            port: mqtt_broker_port(),
            client_id: Some(client_id.clone()),
            ..Default::default()
        },
        topic: OneOrMany::One(topic.to_owned()),
        acknowledgements: true.into(),
        ..MqttSourceConfig::default()
    };

    // Phase 1: the first instance subscribes (creating the persistent session)
    // and receives the message, but its sink rejects every event, so the source
    // never sends a PUBACK.
    let (tx1, mut rx1) = SourceSender::new_test_finalize(EventStatus::Rejected);
    let config1 = make_config();
    let source1 = tokio::spawn(async move {
        config1
            .build(SourceContext::new_test(tx1, None))
            .await
            .unwrap()
            .await
            .unwrap()
    });

    // Wait for the subscription to be established before publishing.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let producer = get_mqtt_client().await;
    producer
        .publish(topic, QoS::AtLeastOnce, false, message.as_bytes())
        .await
        .unwrap();

    // The first instance must actually receive (and reject) the message so that
    // it remains unacknowledged in the broker.
    let first = timeout(Duration::from_secs(5), rx1.next())
        .await
        .expect("timed out waiting for first delivery")
        .expect("source stream ended unexpectedly");
    assert_eq!(message_body(&first), message);
    drop(first);

    // Give the source a moment to observe the rejected status (and therefore
    // skip the ack), then drop the connection without acknowledging.
    tokio::time::sleep(Duration::from_millis(200)).await;
    source1.abort();
    drop(source1.await);

    // Phase 2: a new instance resumes the same session; the broker must
    // redeliver the unacknowledged message.
    let (tx2, mut rx2) = SourceSender::new_test();
    let config2 = make_config();
    let source2 = tokio::spawn(async move {
        config2
            .build(SourceContext::new_test(tx2, None))
            .await
            .unwrap()
            .await
            .unwrap()
    });

    let redelivered = timeout(Duration::from_secs(10), rx2.next())
        .await
        .expect("timed out waiting for redelivery: the message was lost")
        .expect("source stream ended unexpectedly");
    assert_eq!(
        message_body(&redelivered),
        message,
        "redelivered message did not match the original"
    );

    source2.abort();
    drop(source2.await);
}

#[tokio::test]
async fn mqtt_many_topics_happy() {
    trace_init();
    let topic_prefix_1 = "source-prefix-1";
    let topic_prefix_2 = "source-prefix-2";
    // We always want new client ID. If it were stable, subsequent tests could receive data sent in previous runs.
    let client_id = format!("sourceTest{}", random_string(6));
    let num_events = 10;
    let (input_1, ..) = random_lines_with_stream(100, num_events, None);
    let (input_2, ..) = random_lines_with_stream(100, num_events, None);

    assert_source_compliance(&SOURCE_TAGS, async {
        let common = MqttCommonConfig {
            host: mqtt_broker_address(),
            port: mqtt_broker_port(),
            client_id: Some(client_id),
            ..Default::default()
        };

        let config = MqttSourceConfig {
            common,
            topic: OneOrMany::Many(vec![
                format!("{topic_prefix_1}/#"),
                format!("{topic_prefix_2}/#"),
            ]),
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
        send_test_events(&client, &format!("{topic_prefix_1}/test"), &input_1).await;
        send_test_events(&client, &format!("{topic_prefix_2}/test"), &input_2).await;

        let mut expected_messages: HashSet<_> =
            input_1.into_iter().chain(input_2.into_iter()).collect();

        let events: Vec<Event> = timeout(Duration::from_secs(2), rx.take(num_events * 2).collect())
            .await
            .unwrap();

        for event in events {
            let message = event
                .as_log()
                .get(log_schema().message_key_target_path().unwrap())
                .unwrap()
                .to_string_lossy();
            if !expected_messages.remove(message.as_ref()) {
                panic!("Received unexpected message: {message:?}");
            }
        }
        assert!(expected_messages.is_empty());
    })
    .await;
}
