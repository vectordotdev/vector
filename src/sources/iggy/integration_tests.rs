use iggy::prelude::{Client, IggyClient, IggyClientBuilder, IggyExpiry, IggyMessage, MaxTopicSize};
use vector_lib::config::log_schema;

use crate::{
    SourceSender,
    config::{SourceConfig, SourceContext},
    sources::iggy::config::{IggySourceConfig, default_stream_key_field, default_topic_key_field},
    test_util::{
        collect_n,
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
        partition: Some(1),
        batch_length: 100,
        stream_key_field: default_stream_key_field(),
        topic_key_field: default_topic_key_field(),
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
        .create_topic_if_not_exists(
            1,
            None,
            IggyExpiry::ServerDefault,
            MaxTopicSize::ServerDefault,
        )
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
