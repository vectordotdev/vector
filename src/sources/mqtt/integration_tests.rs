#![cfg(feature = "mqtt-integration-tests")]
#![cfg(test)]

use std::{collections::HashSet, time::Duration};

use futures::{Stream, StreamExt};
use rumqttc::{AsyncClient, Event as MqttEvent, Incoming, MqttOptions, QoS};
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

/// Forces a server-side disconnect of whatever client currently holds
/// `client_id`: a second connection with the same client ID makes the broker
/// disconnect the existing one ([MQTT-3.1.4-2]). With `clean_session = false`
/// the persistent session (subscriptions and unacknowledged in-flight
/// messages) survives the takeover; with `clean_session = true` the broker
/// additionally discards that session. The takeover connection is closed
/// immediately afterwards, freeing the client ID for the victim to
/// reconnect.
async fn kick_mqtt_client(client_id: &str, discard_session: bool) {
    let mut options = MqttOptions::new(client_id, mqtt_broker_address(), mqtt_broker_port());
    options.set_keep_alive(Duration::from_secs(5));
    options.set_clean_session(discard_session);
    let (client, mut eventloop) = AsyncClient::new(options, 10);

    // Wait until the broker accepts the takeover (kicking the victim).
    loop {
        match timeout(Duration::from_secs(5), eventloop.poll())
            .await
            .expect("timed out connecting the takeover client")
        {
            Ok(MqttEvent::Incoming(Incoming::ConnAck(_))) => break,
            Ok(_) => {}
            Err(error) => panic!("takeover client failed to connect: {error:?}"),
        }
    }

    client.disconnect().await.unwrap();
    // Drive the DISCONNECT out; polling errors once the connection closes.
    while timeout(Duration::from_secs(5), eventloop.poll())
        .await
        .expect("timed out closing the takeover client")
        .is_ok()
    {}
}

/// Receives from the source until every message in `expected` has been seen
/// at least once (duplicates are permitted: QoS 1 is at-least-once),
/// panicking if `each_timeout` passes without progress.
async fn collect_expected_messages(
    rx: &mut (impl Stream<Item = Event> + Unpin),
    mut expected: HashSet<String>,
    each_timeout: Duration,
) {
    while !expected.is_empty() {
        let event = timeout(each_timeout, rx.next())
            .await
            .unwrap_or_else(|_| panic!("timed out; still missing {} messages", expected.len()))
            .expect("source stream ended unexpectedly");
        expected.remove(&message_body(&event));
    }
}

/// A withheld ack (the sink rejects the message, so its PUBACK is never sent)
/// must force a reconnect on its own -- without an external trigger like a
/// process restart or manual disconnect -- so the broker redelivers the
/// still-unacknowledged publish. Otherwise a broker that only redelivers
/// QoS 1 messages on reconnect (rather than on an independent timer) would
/// leave the source connected but stuck, never receiving it again.
#[tokio::test]
async fn mqtt_forces_reconnect_after_withheld_ack() {
    trace_init();

    let topic = "source-forced-reconnect-test";
    let client_id = format!("sourceForcedReconnectTest{}", random_string(6));
    let message = random_string(32);

    let config = MqttSourceConfig {
        common: MqttCommonConfig {
            host: mqtt_broker_address(),
            port: mqtt_broker_port(),
            client_id: Some(client_id),
            ..Default::default()
        },
        topic: OneOrMany::One(topic.to_owned()),
        acknowledgements: true.into(),
        ..MqttSourceConfig::default()
    };

    // The first delivery attempt for any publish is rejected (withholding its
    // ack); every attempt after that succeeds. So the only way a second
    // delivery of the same message shows up is a broker redelivery following
    // a reconnect the source must have forced itself.
    let (tx, mut rx) = SourceSender::new_test_errors(|count| count == 0);
    let source = tokio::spawn(async move {
        config
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap()
            .await
            .unwrap()
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let producer = get_mqtt_client().await;
    producer
        .publish(topic, QoS::AtLeastOnce, false, message.as_bytes())
        .await
        .unwrap();

    let first = timeout(Duration::from_secs(5), rx.next())
        .await
        .expect("timed out waiting for first delivery")
        .expect("source stream ended unexpectedly");
    assert_eq!(message_body(&first), message);

    let redelivered = timeout(Duration::from_secs(10), rx.next())
        .await
        .expect(
            "timed out waiting for the forced reconnect to trigger redelivery: \
             the source never reconnected on its own",
        )
        .expect("source stream ended unexpectedly");
    assert_eq!(
        message_body(&redelivered),
        message,
        "redelivered message did not match the original"
    );

    source.abort();
    drop(source.await);
}

/// A server-initiated disconnect (broker kicks the client) must be survived
/// transparently when the session is preserved: the source reconnects,
/// resumes the session (`session_present = true`, no resubscribe needed),
/// and messages published while it was offline are delivered from the
/// session's queue.
#[tokio::test]
async fn mqtt_recovers_from_server_side_disconnect() {
    trace_init();

    let topic = "source-server-kick-test";
    let client_id = format!("sourceKickTest{}", random_string(6));
    let msg_before = random_string(32);
    let msg_after = random_string(32);

    let config = MqttSourceConfig {
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

    let (tx, mut rx) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let source = tokio::spawn(async move {
        config
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap()
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(500)).await;

    let producer = get_mqtt_client().await;
    producer
        .publish(topic, QoS::AtLeastOnce, false, msg_before.as_bytes())
        .await
        .unwrap();
    let received = timeout(Duration::from_secs(5), rx.next())
        .await
        .expect("timed out waiting for pre-kick delivery")
        .expect("source stream ended unexpectedly");
    assert_eq!(message_body(&received), msg_before);

    // Kick the source off the broker, keeping its session intact, and
    // publish while it is (briefly) offline: the persistent session queues
    // the message for delivery once the source has reconnected.
    kick_mqtt_client(&client_id, false).await;
    producer
        .publish(topic, QoS::AtLeastOnce, false, msg_after.as_bytes())
        .await
        .unwrap();

    let redelivered = timeout(Duration::from_secs(10), rx.next())
        .await
        .expect("timed out: the source did not recover from the server-side disconnect")
        .expect("source stream ended unexpectedly");
    assert_eq!(message_body(&redelivered), msg_after);

    source.abort();
    drop(source.await);
}

/// When the broker discards the persistent session entirely (here: a
/// takeover connection with `clean_session = true`, but a broker restart
/// without persistence behaves the same), the source's reconnect sees
/// `session_present = false` for a session with no subscriptions and must
/// resubscribe on its own before messages flow again.
#[tokio::test]
async fn mqtt_resubscribes_after_broker_discards_the_session() {
    trace_init();

    let topic = "source-session-loss-test";
    let client_id = format!("sourceSessionLossTest{}", random_string(6));
    let msg_before = random_string(32);
    let msg_after = random_string(32);

    let config = MqttSourceConfig {
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

    let (tx, mut rx) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let source = tokio::spawn(async move {
        config
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap()
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(500)).await;

    let producer = get_mqtt_client().await;
    producer
        .publish(topic, QoS::AtLeastOnce, false, msg_before.as_bytes())
        .await
        .unwrap();
    let received = timeout(Duration::from_secs(5), rx.next())
        .await
        .expect("timed out waiting for pre-kick delivery")
        .expect("source stream ended unexpectedly");
    assert_eq!(message_body(&received), msg_before);

    // Kick the source AND discard its session (subscriptions included).
    kick_mqtt_client(&client_id, true).await;

    // A publish only reaches the source once it has reconnected and its
    // resubscribe has been processed; QoS 1 publishes to a topic with no
    // subscribers are simply dropped, so publish repeatedly until one lands
    // rather than guessing at the resubscribe timing.
    let mut received_after = None;
    for _ in 0..20 {
        producer
            .publish(topic, QoS::AtLeastOnce, false, msg_after.as_bytes())
            .await
            .unwrap();
        if let Ok(event) = timeout(Duration::from_millis(500), rx.next()).await {
            received_after = event;
            break;
        }
    }
    let received_after = received_after
        .expect("the source never resubscribed after the broker discarded its session");
    assert_eq!(message_body(&received_after), msg_after);

    source.abort();
    drop(source.await);
}

/// The default mode (acknowledgements off, rumqttc auto-acks) must not lose
/// messages across a server-side disconnect either: in this mode a buffered
/// publish can already be acknowledged to the broker before the source
/// processes it, so anything the disconnect path drops locally would be gone
/// for good (the broker won't redeliver what it saw acked).
#[tokio::test]
async fn mqtt_does_not_lose_messages_across_server_side_disconnect_without_acks() {
    trace_init();

    let topic = "source-kick-no-acks-test";
    let client_id = format!("sourceKickNoAcksTest{}", random_string(6));
    let num_messages = 10;

    let config = MqttSourceConfig {
        common: MqttCommonConfig {
            host: mqtt_broker_address(),
            port: mqtt_broker_port(),
            client_id: Some(client_id.clone()),
            ..Default::default()
        },
        topic: OneOrMany::One(topic.to_owned()),
        ..MqttSourceConfig::default()
    };

    let (tx, mut rx) = SourceSender::new_test();
    let source = tokio::spawn(async move {
        config
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap()
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(500)).await;

    let producer = get_mqtt_client().await;

    let (batch_before, ..) = random_lines_with_stream(100, num_messages, None);
    send_test_events(&producer, topic, &batch_before).await;
    collect_expected_messages(
        &mut rx,
        batch_before.into_iter().collect(),
        Duration::from_secs(5),
    )
    .await;

    // Kick the source (session preserved) and publish the second batch while
    // it is offline; the session queues it for delivery after reconnect.
    kick_mqtt_client(&client_id, false).await;
    let (batch_after, ..) = random_lines_with_stream(100, num_messages, None);
    send_test_events(&producer, topic, &batch_after).await;

    collect_expected_messages(
        &mut rx,
        batch_after.into_iter().collect(),
        Duration::from_secs(10),
    )
    .await;

    source.abort();
    drop(source.await);
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
