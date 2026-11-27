use futures::StreamExt;
use iggy::prelude::{Client, IggyClient, IggyClientBuilder, IggyExpiry, IggyMessage, MaxTopicSize};
use tokio::time::{Duration, Instant};
use vector_lib::config::log_schema;

use crate::{
    SourceSender,
    config::{ComponentKey, SourceConfig, SourceContext},
    sources::iggy::config::IggySourceConfig,
    test_util::{
        collect_n, collect_n_stream,
        components::{SOURCE_TAGS, assert_source_compliance},
        random_string,
    },
};

fn iggy_url() -> String {
    std::env::var("IGGY_ADDRESS").unwrap_or_else(|_| "iggy+tcp://iggy:iggy@127.0.0.1:8090".into())
}

fn generate_source_config(
    url: &str,
    stream: &str,
    topic: &str,
    consumer_name: &str,
) -> IggySourceConfig {
    IggySourceConfig {
        url: url.into(),
        stream: stream.into(),
        topic: topic.into(),
        consumer_name: consumer_name.into(),
        partition: Some(0),
        batch_length: 100,
        ..Default::default()
    }
}

async fn build_admin_client(url: &str) -> IggyClient {
    let client = IggyClientBuilder::from_connection_string(url)
        .expect("invalid connection string")
        .build()
        .expect("failed to build admin client");
    client.connect().await.expect("failed to connect admin");
    client
}

async fn publish_messages(
    client: &IggyClient,
    stream: &str,
    topic: &str,
    payloads: &[&'static str],
) {
    let producer = client
        .producer(stream, topic)
        .expect("producer build failed")
        .create_stream_if_not_exists()
        .create_topic_if_not_exists(1, IggyExpiry::ServerDefault, MaxTopicSize::ServerDefault)
        .build();
    producer.init().await.expect("producer init failed");

    let messages = payloads
        .iter()
        .map(|payload| {
            IggyMessage::builder()
                .payload((*payload).into())
                .build()
                .expect("failed to build IggyMessage")
        })
        .collect();
    producer.send(messages).await.expect("publish failed");
}

#[tokio::test]
async fn iggy_consume_round_trip() {
    let url = iggy_url();
    let stream = format!("src-test-{}", random_string(8));
    let topic = format!("logs-{}", random_string(8));
    let consumer_name = format!("vector-{}", random_string(8));

    // Pre-publish messages so the source has something to consume on init.
    let admin = build_admin_client(&url).await;
    let payloads = ["msg-1", "msg-2", "msg-3"];
    publish_messages(&admin, &stream, &topic, &payloads).await;

    let conf = generate_source_config(&url, &stream, &topic, &consumer_name);

    let events = assert_source_compliance(&SOURCE_TAGS, async move {
        let (tx, rx) = SourceSender::new_test();
        let cx = SourceContext::new_test(tx, None);
        let source = conf.build(cx).await.expect("source build failed");
        tokio::spawn(source);
        collect_n(rx, payloads.len()).await
    })
    .await;

    let message_key = log_schema().message_key().unwrap().to_string();
    let received: Vec<String> = events
        .iter()
        .map(|event| {
            event.as_log()[message_key.clone()]
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    assert_eq!(received, payloads);
}

#[tokio::test]
async fn iggy_consume_round_trip_with_acknowledgements() {
    let url = iggy_url();
    let stream = format!("src-ack-test-{}", random_string(8));
    let topic = format!("logs-{}", random_string(8));
    let consumer_name = format!("vector-ack-{}", random_string(8));

    // Pre-publish messages so the source has something to consume on init.
    let admin = build_admin_client(&url).await;
    let payloads = ["ack-1", "ack-2", "ack-3"];
    publish_messages(&admin, &stream, &topic, &payloads).await;

    let source_id = ComponentKey::from("iggy_ack_round_trip");
    let (tx, rx) = SourceSender::new_test();
    let (mut cx, mut shutdown) = SourceContext::new_shutdown(&source_id, tx);
    cx.acknowledgements = true;

    let source = generate_source_config(&url, &stream, &topic, &consumer_name)
        .build(cx)
        .await
        .expect("source build failed");
    let handle = tokio::spawn(source);

    let mut rx = rx;
    let mut events = collect_n_stream(&mut rx, payloads.len()).await;
    let message_key = log_schema().message_key().unwrap().to_string();
    let received: Vec<String> = events
        .iter()
        .map(|event| {
            event.as_log()[message_key.clone()]
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(received, payloads);

    // A batch sink acknowledges only after it flushes, which the source
    // triggers by closing its output. Holding the events until EOF makes the
    // test fail if the source drains before closing: the drain deadline then
    // expires and the offsets are never committed.
    let deadline = Instant::now() + Duration::from_secs(10);
    let shutdown = shutdown.shutdown_source(&source_id, deadline);
    tokio::pin!(shutdown);
    let mut completed = false;
    let mut settled = false;
    while !(completed && settled) {
        tokio::select! {
            item = rx.next(), if !settled => {
                if let Some(event) = item {
                    events.push(event);
                } else {
                    drop(std::mem::take(&mut events));
                    settled = true;
                }
            }
            result = &mut shutdown, if !completed => {
                assert!(result, "source did not shut down before the deadline");
                completed = true;
            }
        }
    }
    handle
        .await
        .expect("source task panicked")
        .expect("source reported failure on graceful shutdown");

    // A consumer with the same name and partition must resume after the
    // committed offsets instead of consuming the delivered messages again.
    let restart_id = ComponentKey::from("iggy_ack_round_trip_restart");
    let (tx, mut rx) = SourceSender::new_test();
    let (mut cx, mut shutdown) = SourceContext::new_shutdown(&restart_id, tx);
    cx.acknowledgements = true;
    let source = generate_source_config(&url, &stream, &topic, &consumer_name)
        .build(cx)
        .await
        .expect("source rebuild failed");
    let handle = tokio::spawn(source);

    let replayed = tokio::time::timeout(Duration::from_secs(2), rx.next()).await;
    assert!(
        replayed.is_err(),
        "delivered messages were replayed after restart: {replayed:?}"
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    assert!(shutdown.shutdown_source(&restart_id, deadline).await);
    handle
        .await
        .expect("source task panicked")
        .expect("source reported failure on graceful shutdown");
}
