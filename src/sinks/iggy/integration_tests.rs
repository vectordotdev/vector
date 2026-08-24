use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use iggy::prelude::{
    Client, IggyClient, IggyClientBuilder, PollOptions, StreamClient, TopicClient,
};
use vector_lib::codecs::TextSerializerConfig;

use super::{
    IggyError,
    config::{Connection, IggySinkConfig, IggyTowerRequestConfigDefaults},
    sink::IggySink,
};
use crate::{
    sinks::prelude::*,
    test_util::{
        components::{SINK_TAGS, run_and_assert_sink_compliance},
        random_lines_with_stream, random_string, trace_init,
    },
};

fn iggy_url() -> String {
    std::env::var("IGGY_ADDRESS").unwrap_or_else(|_| "iggy+tcp://iggy:iggy@127.0.0.1:8090".into())
}

fn generate_sink_config(url: &str, stream: &str, topic: &str) -> IggySinkConfig {
    IggySinkConfig {
        url: url.into(),
        stream: stream.into(),
        topic: topic.into(),
        partitions: 1,
        encoding: TextSerializerConfig::default().into(),
        acknowledgements: AcknowledgementsConfig::default(),
        request: TowerRequestConfig::<IggyTowerRequestConfigDefaults>::default(),
        batch: BatchConfig::default(),
    }
}

async fn build_test_client(url: &str) -> IggyClient {
    let client = IggyClientBuilder::from_connection_string(url)
        .expect("invalid connection string")
        .build()
        .expect("failed to build verifier client");
    client.connect().await.expect("failed to connect verifier");
    client
}

async fn publish_and_check(conf: IggySinkConfig) -> Result<(), IggyError> {
    let verifier_url = conf.url.clone();
    let connection = Arc::new(Connection::new(&conf));
    connection.get().await?;
    let batcher_settings = conf
        .batch
        .validate()
        .expect("test batch settings are valid")
        .into_batcher_settings()
        .expect("test batch settings are valid");
    let sink =
        VectorSink::from_event_streamsink(IggySink::new(&conf, batcher_settings, connection)?);

    let num_events = 10;
    let (input, events) = random_lines_with_stream(100, num_events, None);

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

    let verifier = build_test_client(&verifier_url).await;
    let mut consumer = verifier
        .consumer(
            &format!("verify-{}", random_string(8)),
            &conf.stream,
            &conf.topic,
        )
        .expect("failed to create verifier consumer")
        .partitions([0])
        .poll_options(PollOptions {
            count: u32::try_from(num_events).expect("event count fits in u32"),
            ..Default::default()
        })
        .build();
    consumer
        .init()
        .await
        .expect("failed to init verifier consumer");

    let mut output = Vec::with_capacity(num_events);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while output.len() < num_events {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, consumer.next()).await {
            Ok(Some(Ok(received))) => {
                output.push(String::from_utf8_lossy(&received.message.payload).to_string());
            }
            Ok(Some(Err(error))) => panic!("verifier read failed: {error}"),
            Ok(None) => panic!("verifier stream ended before every message arrived"),
            Err(_) => break,
        }
    }

    assert_eq!(output, input);
    Ok(())
}

#[tokio::test]
async fn iggy_publish_round_trip() {
    trace_init();

    let stream = format!("vector-test-{}", random_string(8));
    let topic = format!("logs-{}", random_string(8));
    let conf = generate_sink_config(&iggy_url(), &stream, &topic);

    publish_and_check(conf)
        .await
        .expect("publish_and_check failed");
}

#[tokio::test]
async fn iggy_creates_stream_and_topic_on_connect() {
    trace_init();

    let url = iggy_url();
    let stream = format!("auto-create-{}", random_string(8));
    let topic = format!("logs-{}", random_string(8));

    let conf = generate_sink_config(&url, &stream, &topic);
    Connection::new(&conf)
        .get()
        .await
        .expect("connection failed");

    let verifier = build_test_client(&url).await;
    let stream_details = verifier
        .get_stream(&stream.as_str().try_into().unwrap())
        .await
        .expect("get_stream failed");
    assert!(stream_details.is_some(), "stream was not created");

    let topic_details = verifier
        .get_topic(
            &stream.as_str().try_into().unwrap(),
            &topic.as_str().try_into().unwrap(),
        )
        .await
        .expect("get_topic failed");
    assert!(topic_details.is_some(), "topic was not created");

    // A fresh connection against the same stream/topic must succeed even
    // though both already exist.
    Connection::new(&generate_sink_config(&url, &stream, &topic))
        .get()
        .await
        .expect("idempotent re-init failed");
}
