#![cfg(feature = "mqtt-integration-tests")]
#![cfg(test)]

use std::{collections::HashSet, time::Duration};

use futures::{Stream, StreamExt};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde_json::{Value, json};
use tokio::time::timeout;

use crate::{
    SourceSender,
    common::mqtt::MqttCommonConfig,
    config::{SourceConfig, SourceContext, log_schema},
    event::Event,
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

fn emqx_api_url() -> String {
    std::env::var("EMQX_API_URL")
        .unwrap_or_else(|_| format!("http://{}:18083/api/v5", mqtt_broker_address()))
}

async fn send_test_events(client: &AsyncClient, topic: &str, messages: &Vec<String>) {
    for message in messages {
        client
            .publish(topic, QoS::AtLeastOnce, false, message.as_bytes())
            .await
            .unwrap();
    }
}

fn get_mqtt_client() -> AsyncClient {
    let mut mqtt_options = MqttOptions::new(
        format!("integration-test-producer-{}", random_string(6)),
        mqtt_broker_address(),
        mqtt_broker_port(),
    );
    mqtt_options.set_keep_alive(5);

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
                .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = get_mqtt_client();
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
            assert!(
                expected_messages.remove(message.as_ref()),
                "Received unexpected message: {message:?}"
            );
        }
        assert!(expected_messages.is_empty());
    })
    .await;
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
                .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = get_mqtt_client();
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
            assert!(
                expected_messages.remove(message.as_ref()),
                "Received unexpected message: {message:?}"
            );
        }
        assert!(expected_messages.is_empty());
    })
    .await;
}

async fn emqx_client_session(http: &reqwest::Client, token: &str, client_id: &str) -> Value {
    http.get(format!("{}/clients/{client_id}", emqx_api_url()))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// Kicks the client through the EMQX REST API, which also discards its session, and waits until
/// it has reconnected with a new one.
async fn discard_session(client_id: &str) {
    let http = reqwest::Client::new();
    let login: Value = http
        .post(format!("{}/login", emqx_api_url()))
        .json(&json!({"username": "admin", "password": "public"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = login["token"].as_str().unwrap();

    let created_at = emqx_client_session(&http, token, client_id).await["created_at"].clone();

    let status = http
        .delete(format!("{}/clients/{client_id}", emqx_api_url()))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "Kicking the client failed: {status}");

    timeout(Duration::from_secs(10), async {
        loop {
            let session = emqx_client_session(&http, token, client_id).await;
            if session["connected"] == true && session["created_at"] != created_at {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Source did not reconnect");
}

async fn assert_receives(rx: &mut (impl Stream<Item = Event> + Unpin), messages: Vec<String>) {
    let count = messages.len();
    let mut expected_messages: HashSet<_> = messages.into_iter().collect();

    let events: Vec<Event> = timeout(Duration::from_secs(2), rx.take(count).collect())
        .await
        .unwrap();

    for event in events {
        let message = event
            .as_log()
            .get(log_schema().message_key_target_path().unwrap())
            .unwrap()
            .to_string_lossy();
        assert!(
            expected_messages.remove(message.as_ref()),
            "Received unexpected message: {message:?}"
        );
    }
    assert!(expected_messages.is_empty());
}

#[tokio::test]
async fn mqtt_resubscribes_after_session_loss() {
    trace_init();
    let topic = "source-resubscribe";
    let client_id = format!("sourceTest{}", random_string(6));
    let num_events = 10;
    let (before, ..) = random_lines_with_stream(100, num_events, None);
    let (after, ..) = random_lines_with_stream(100, num_events, None);

    assert_source_compliance(&SOURCE_TAGS, async {
        let common = MqttCommonConfig {
            host: mqtt_broker_address(),
            port: mqtt_broker_port(),
            client_id: Some(client_id.clone()),
            ..Default::default()
        };

        let config = MqttSourceConfig {
            common,
            topic: OneOrMany::One(topic.to_owned()),
            ..MqttSourceConfig::default()
        };

        let (tx, mut rx) = SourceSender::new_test();
        tokio::spawn(async move {
            config
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap()
                .await
                .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = get_mqtt_client();
        send_test_events(&client, topic, &before).await;
        assert_receives(&mut rx, before).await;

        discard_session(&client_id).await;

        send_test_events(&client, topic, &after).await;
        assert_receives(&mut rx, after).await;
    })
    .await;
}
