use std::time::Duration;

use chrono::{DateTime, SubsecRound, Utc};
use futures::Stream;
use futures_util::stream::FuturesUnordered;
use rdkafka::{
    Offset, TopicPartitionList,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    config::{ClientConfig, FromClientConfig},
    consumer::BaseConsumer,
    message::{Header, OwnedHeaders},
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};
use stream_cancel::{Trigger, Tripwire};
use tokio::time::sleep;
use vector_lib::event::EventStatus;
use vrl::{event_path, value};

use super::{test::*, *};
use crate::{
    SourceSender,
    event::{EventArray, EventContainer},
    shutdown::ShutdownSignal,
    test_util::{collect_n, components::assert_source_compliance, random_string},
};

const KEY: &str = "my key";
const TEXT: &str = "my message";
const HEADER_KEY: &str = "my header";
const HEADER_VALUE: &str = "my header value";

fn message_indices(events: &[Event]) -> HashSet<usize> {
    events
        .iter()
        .map(|event| {
            let key = event.as_log()["message_key"].to_string_lossy();
            key.strip_prefix(&format!("{KEY} "))
                .expect("message_key should have the expected prefix")
                .parse()
                .expect("message_key suffix should be the message index")
        })
        .collect()
}

fn message_offsets(events: &[Event]) -> HashSet<(i64, i64)> {
    events
        .iter()
        .map(|event| {
            let log = event.as_log();
            let partition = log["partition"].to_string_lossy().parse().expect(
                "partition should be an integer, since it was pulled from a `sources::kafka::Keys.partition` field",
            );
            let offset = log["offset"].to_string_lossy().parse().expect(
                "offset should be an integer, since it was pulled from a `sources::kafka::Keys.offset` field",
            );
            (partition, offset)
        })
        .collect()
}

fn kafka_test_topic() -> String {
    std::env::var("KAFKA_TEST_TOPIC")
        .unwrap_or_else(|_| format!("test-topic-{}", random_string(10)))
}
fn kafka_max_bytes() -> String {
    std::env::var("KAFKA_MAX_BYTES").unwrap_or_else(|_| "1024".into())
}

fn client_config<T: FromClientConfig>(group: Option<&str>) -> T {
    let mut client = ClientConfig::new();
    client.set("bootstrap.servers", kafka_address());
    client.set("produce.offset.report", "true");
    client.set("message.timeout.ms", "5000");
    client.set("auto.commit.interval.ms", "1");
    if let Some(group) = group {
        client.set("group.id", group);
    }
    client.create().expect("Producer creation error")
}

async fn send_events(topic: String, partitions: i32, count: usize) -> DateTime<Utc> {
    let now = Utc::now();
    let timestamp = now.timestamp_millis();

    let producer: &FutureProducer = &client_config(None);
    let topic_name = topic.as_ref();

    create_topic(topic_name, partitions).await;

    (0..count)
        .map(|i| async move {
            let text = format!("{TEXT} {i:03}");
            let key = format!("{KEY} {i}");
            let record = FutureRecord::to(topic_name)
                .payload(&text)
                .key(&key)
                .timestamp(timestamp)
                .headers(OwnedHeaders::new().insert(Header {
                    key: HEADER_KEY,
                    value: Some(HEADER_VALUE),
                }));
            if let Err(error) = producer.send(record, Timeout::Never).await {
                panic!("Cannot send event to Kafka: {error:?}");
            }
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await;

    now
}

async fn send_to_test_topic(partitions: i32, count: usize) -> (String, String, DateTime<Utc>) {
    let topic = kafka_test_topic();
    let group_id = format!("test-group-{}", random_string(10));

    let sent_at = send_events(topic.clone(), partitions, count).await;

    (topic, group_id, sent_at)
}

#[tokio::test]
async fn consumes_event_with_acknowledgements() {
    send_receive(true, |_| false, 10, LogNamespace::Legacy).await;
}

#[tokio::test]
async fn consumes_event_with_acknowledgements_vector_namespace() {
    send_receive(true, |_| false, 10, LogNamespace::Vector).await;
}

#[tokio::test]
async fn consumes_event_without_acknowledgements() {
    send_receive(false, |_| false, 10, LogNamespace::Legacy).await;
}

#[tokio::test]
async fn consumes_event_without_acknowledgements_vector_namespace() {
    send_receive(false, |_| false, 10, LogNamespace::Vector).await;
}

#[tokio::test]
async fn handles_one_negative_acknowledgement() {
    send_receive(true, |n| n == 2, 10, LogNamespace::Legacy).await;
}

#[tokio::test]
async fn handles_one_negative_acknowledgement_vector_namespace() {
    send_receive(true, |n| n == 2, 10, LogNamespace::Vector).await;
}

#[tokio::test]
async fn handles_permanent_negative_acknowledgement() {
    send_receive(true, |n| n >= 2, 2, LogNamespace::Legacy).await;
}

#[tokio::test]
async fn handles_permanent_negative_acknowledgement_vector_namespace() {
    send_receive(true, |n| n >= 2, 2, LogNamespace::Vector).await;
}

fn train_test_dictionary() -> Vec<u8> {
    let samples: Vec<Vec<u8>> = (0..100)
        .map(|i| format!("{TEXT} sample {i:04}").into_bytes())
        .collect();
    zstd::dict::from_samples(&samples, 4 * 1024).expect("failed to train dictionary")
}

fn write_test_dictionary(dictionary: &[u8]) -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().expect("failed to create dictionary file");
    std::fs::write(file.path(), dictionary).expect("failed to write dictionary");
    file
}

/// Produces `count` messages whose payloads are compressed with `dictionary`, except for the
/// message at `corrupt_index`, whose payload is not valid zstd data.
async fn send_dictionary_compressed_events(
    topic: &str,
    dictionary: &[u8],
    count: usize,
    corrupt_index: usize,
) {
    create_topic(topic, 1).await;

    let mut compressor = zstd::bulk::Compressor::with_dictionary(3, dictionary)
        .expect("failed to create compressor");
    let producer: &FutureProducer = &client_config(None);
    for i in 0..count {
        let text = format!("{TEXT} {i:03}");
        let payload = if i == corrupt_index {
            b"definitely not zstd".to_vec()
        } else {
            compressor
                .compress(text.as_bytes())
                .expect("failed to compress payload")
        };
        let key = format!("{KEY} {i}");
        let record = FutureRecord::to(topic).payload(&payload).key(&key);
        if let Err(error) = producer.send(record, Timeout::Never).await {
            panic!("Cannot send event to Kafka: {error:?}");
        }
    }
}

#[tokio::test]
async fn consumes_zstd_dictionary_compressed_payloads() {
    const SEND_COUNT: usize = 5;
    const CORRUPT_INDEX: usize = 2;

    let topic = format!("test-topic-{}", random_string(10));
    let group_id = format!("test-group-{}", random_string(10));

    let dictionary = train_test_dictionary();
    let dictionary_file = write_test_dictionary(&dictionary);

    let mut config = make_config(&topic, &group_id, LogNamespace::Legacy, None);
    config.decompression = Some(DecompressionConfig {
        algorithm: vector_lib::codecs::DecompressionAlgorithm::Zstd,
        dictionary_path: Some(dictionary_file.path().to_path_buf()),
    });

    // Produce messages whose payloads are compressed with the dictionary, plus one corrupt
    // payload that must be skipped without stalling the partition.
    send_dictionary_compressed_events(&topic, &dictionary, SEND_COUNT, CORRUPT_INDEX).await;

    let events = assert_source_compliance(&["protocol", "topic", "partition"], async move {
        let (tx, rx) = SourceSender::new_test_errors(|_| false);
        let (trigger_shutdown, shutdown_done) =
            spawn_kafka(tx, config, false, false, LogNamespace::Legacy);
        let events = collect_n(rx, SEND_COUNT - 1).await;
        tokio::task::yield_now().await;
        drop(trigger_shutdown);
        shutdown_done.await;

        events
    })
    .await;

    assert_eq!(events.len(), SEND_COUNT - 1);
    let messages: HashSet<String> = events
        .iter()
        .map(|event| {
            event.as_log()[log_schema().message_key().unwrap().to_string()]
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let expected: HashSet<String> = (0..SEND_COUNT)
        .filter(|i| *i != CORRUPT_INDEX)
        .map(|i| format!("{TEXT} {i:03}"))
        .collect();
    assert_eq!(messages, expected);
}

#[tokio::test]
async fn advances_offset_past_poison_messages() {
    const SEND_COUNT: usize = 5;

    let topic = format!("test-topic-{}", random_string(10));
    let group_id = format!("test-group-{}", random_string(10));

    let dictionary = train_test_dictionary();
    let dictionary_file = write_test_dictionary(&dictionary);

    let mut opts = HashMap::new();
    opts.insert("enable.partition.eof".into(), "true".into());
    let mut config = make_config(&topic, &group_id, LogNamespace::Legacy, Some(opts));
    config.decompression = Some(DecompressionConfig {
        algorithm: vector_lib::codecs::DecompressionAlgorithm::Zstd,
        dictionary_path: Some(dictionary_file.path().to_path_buf()),
    });

    // Corrupt message is last on the partition to ensure a later good message doesn't commit past it.
    send_dictionary_compressed_events(&topic, &dictionary, SEND_COUNT, SEND_COUNT - 1).await;

    let events = assert_source_compliance(&["protocol", "topic", "partition"], async move {
        let (tx, rx) = SourceSender::new_test_errors(|_| false);
        let (trigger_shutdown, shutdown_done) =
            spawn_kafka(tx, config, true, true, LogNamespace::Legacy);
        // With `eof` enabled the source exits by itself once the partition is fully consumed.
        let events = rx.collect::<Vec<Event>>().await;
        drop(trigger_shutdown);
        shutdown_done.await;

        events
    })
    .await;

    assert_eq!(events.len(), SEND_COUNT - 1);
    assert_eq!(
        fetch_tpl_offset(&group_id, &topic, 0),
        Offset::from_raw(SEND_COUNT as i64)
    );
}

async fn send_receive(
    acknowledgements: bool,
    error_at: impl Fn(usize) -> bool,
    receive_count: usize,
    log_namespace: LogNamespace,
) {
    const SEND_COUNT: usize = 10;

    let topic = format!("test-topic-{}", random_string(10));
    let group_id = format!("test-group-{}", random_string(10));
    let config = make_config(&topic, &group_id, log_namespace, None);

    let now = send_events(topic.clone(), 1, 10).await;

    let events = assert_source_compliance(&["protocol", "topic", "partition"], async move {
        let (tx, rx) = SourceSender::new_test_errors(error_at);
        let (trigger_shutdown, shutdown_done) =
            spawn_kafka(tx, config, acknowledgements, false, log_namespace);
        let events = collect_n(rx, SEND_COUNT).await;
        // Yield to the finalization task to let it collect the
        // batch status receivers before signalling the shutdown.
        tokio::task::yield_now().await;
        drop(trigger_shutdown);
        shutdown_done.await;

        events
    })
    .await;

    let offset = fetch_tpl_offset(&group_id, &topic, 0);
    assert_eq!(offset, Offset::from_raw(receive_count as i64));

    assert_eq!(events.len(), SEND_COUNT);
    for (i, event) in events.into_iter().enumerate() {
        if let LogNamespace::Legacy = log_namespace {
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                format!("{TEXT} {i:03}").into()
            );
            assert_eq!(event.as_log()["message_key"], format!("{KEY} {i}").into());
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "kafka".into()
            );
            assert_eq!(
                event.as_log()[log_schema().timestamp_key().unwrap().to_string()],
                now.trunc_subsecs(3).into()
            );
            assert_eq!(event.as_log()["topic"], topic.clone().into());
            assert!(event.as_log().contains(event_path!("partition")));
            assert!(event.as_log().contains(event_path!("offset")));
            let mut expected_headers = ObjectMap::new();
            expected_headers.insert(HEADER_KEY.into(), Value::from(HEADER_VALUE));
            assert_eq!(event.as_log()["headers"], Value::from(expected_headers));
        } else {
            let meta = event.as_log().metadata().value();

            assert_eq!(
                meta.get(path!("vector", "source_type")).unwrap(),
                &value!(KafkaSourceConfig::NAME)
            );
            assert!(
                meta.get(path!("vector", "ingest_timestamp"))
                    .unwrap()
                    .is_timestamp()
            );

            assert_eq!(
                event.as_log().value(),
                &value!(format!("{} {:03}", TEXT, i))
            );
            assert_eq!(
                meta.get(path!("kafka", "message_key")).unwrap(),
                &value!(format!("{} {}", KEY, i))
            );

            assert_eq!(
                meta.get(path!("kafka", "timestamp")).unwrap(),
                &value!(now.trunc_subsecs(3))
            );
            assert_eq!(
                meta.get(path!("kafka", "topic")).unwrap(),
                &value!(topic.clone())
            );
            assert!(meta.get(path!("kafka", "partition")).unwrap().is_integer(),);
            assert!(meta.get(path!("kafka", "offset")).unwrap().is_integer(),);

            let mut expected_headers = ObjectMap::new();
            expected_headers.insert(HEADER_KEY.into(), Value::from(HEADER_VALUE));
            assert_eq!(
                meta.get(path!("kafka", "headers")).unwrap(),
                &Value::from(expected_headers)
            );
        }
    }
}

fn make_rand_config() -> (String, String, KafkaSourceConfig) {
    let topic = format!("test-topic-{}", random_string(10));
    let group_id = format!("test-group-{}", random_string(10));
    let config = make_config(&topic, &group_id, LogNamespace::Legacy, None);
    (topic, group_id, config)
}

fn delay_pipeline(
    id: usize,
    delay: Duration,
    status: EventStatus,
) -> (SourceSender, impl Stream<Item = EventArray> + Unpin) {
    let (pipe, recv) = SourceSender::new_test_sender_with_options(100, None);
    let recv = recv.into_stream();
    let recv = recv.then(move |item| async move {
        let mut events = item.events;
        events.iter_logs_mut().for_each(|log| {
            log.insert(event_path!("pipeline_id"), id.to_string());
        });
        sleep(delay).await;
        events.iter_events_mut().for_each(|mut event| {
            let metadata = event.metadata_mut();
            metadata.update_status(status);
            metadata.update_sources();
        });
        events
    });
    (pipe, Box::pin(recv))
}

fn spawn_kafka(
    out: SourceSender,
    config: KafkaSourceConfig,
    acknowledgements: bool,
    eof: bool,
    log_namespace: LogNamespace,
) -> (Trigger, Tripwire) {
    let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();

    let decoder = DecodingConfig::new(
        config.framing.clone(),
        config.decoding.clone(),
        log_namespace,
    )
    .build()
    .unwrap();

    let decompressor = config
        .decompression
        .as_ref()
        .map(DecompressionConfig::build)
        .transpose()
        .unwrap();

    let (consumer, callback_rx) = create_consumer(&config, acknowledgements).unwrap();

    tokio::spawn(kafka_source(
        config,
        consumer,
        callback_rx,
        decoder,
        decompressor,
        out,
        shutdown,
        eof,
        log_namespace,
    ));
    (trigger_shutdown, shutdown_done)
}

fn fetch_tpl_offset(group_id: &str, topic: &str, partition: i32) -> Offset {
    let client: BaseConsumer = client_config(Some(group_id));
    client.subscribe(&[topic]).expect("Subscribing failed");

    let mut tpl = TopicPartitionList::new();
    tpl.add_partition(topic, partition);
    client
        .committed_offsets(tpl, Duration::from_secs(1))
        .expect("Getting committed offsets failed")
        .find_partition(topic, partition)
        .expect("Missing topic/partition")
        .offset()
}

async fn create_topic(topic: &str, partitions: i32) {
    let client: AdminClient<DefaultClientContext> = client_config(None);
    let topic_results = client
        .create_topics(
            [&NewTopic {
                name: topic,
                num_partitions: partitions,
                replication: TopicReplication::Fixed(1),
                config: vec![],
            }],
            &AdminOptions::default(),
        )
        .await
        .expect("create_topics failed");

    for result in topic_results {
        if let Err((topic, err)) = result
            && err != rdkafka::types::RDKafkaErrorCode::TopicAlreadyExists
        {
            panic!("Creating a topic failed: {:?}", (topic, err))
        }
    }
}

// Failure timeline:
// - Topic exists on multiple partitions
// - Consumer A connects to topic, is assigned both partitions
// - Consumer A receives some messages
// - Consumer B connects to topic
// - Consumer A has one partition revoked (rebalance)
// - Consumer B is assigned a partition
// - Consumer A stores an order on the revoked partition
// - Consumer B skips receiving messages?
#[ignore]
#[tokio::test]
async fn handles_rebalance() {
    // The test plan here is to:
    // - Set up one source instance, feeding into a pipeline that delays acks.
    // - Wait a bit, and set up a second source instance. This should cause a rebalance.
    // - Wait further until all events will have been pulled down.
    // - Verify that all events are captured by the two sources, and that offsets are set right, etc.

    // However this test, as written, does not actually cause the
    // conditions required to test this. We have had external
    // validation that the sink behaves properly on rebalance
    // events.  This test also requires the insertion of a small
    // delay into the source to guarantee the timing, which is not
    // suitable for production code.

    const NEVENTS: usize = 200;
    const DELAY: u64 = 100;

    let (topic, group_id, config) = make_rand_config();
    create_topic(&topic, 2).await;

    let _send_start = send_events(topic.clone(), 1, NEVENTS).await;

    let (tx, rx1) = delay_pipeline(1, Duration::from_millis(200), EventStatus::Delivered);
    let (trigger_shutdown1, shutdown_done1) =
        spawn_kafka(tx, config.clone(), true, false, LogNamespace::Legacy);
    let events1 = tokio::spawn(collect_n(rx1, NEVENTS));

    sleep(Duration::from_secs(1)).await;

    let (tx, rx2) = delay_pipeline(2, Duration::from_millis(DELAY), EventStatus::Delivered);
    let (trigger_shutdown2, shutdown_done2) =
        spawn_kafka(tx, config, true, false, LogNamespace::Legacy);
    let events2 = tokio::spawn(collect_n(rx2, NEVENTS));

    sleep(Duration::from_secs(5)).await;

    drop(trigger_shutdown1);
    let events1 = events1.await.unwrap();
    shutdown_done1.await;

    sleep(Duration::from_secs(5)).await;

    drop(trigger_shutdown2);
    let events2 = events2.await.unwrap();
    shutdown_done2.await;

    sleep(Duration::from_secs(1)).await;

    assert!(!events1.is_empty());
    assert!(!events2.is_empty());

    match fetch_tpl_offset(&group_id, &topic, 0) {
        Offset::Offset(offset) => {
            assert!((offset as isize - events1.len() as isize).abs() <= 1)
        }
        o => panic!("Invalid offset for partition 0 {o:?}"),
    }

    match fetch_tpl_offset(&group_id, &topic, 1) {
        Offset::Offset(offset) => {
            assert!((offset as isize - events2.len() as isize).abs() <= 1)
        }
        o => panic!("Invalid offset for partition 0 {o:?}"),
    }

    let mut all_events = events1
        .into_iter()
        .chain(events2.into_iter())
        .flat_map(map_logs)
        .collect::<Vec<String>>();
    all_events.sort();

    // Assert they are all in sequential order and no dupes, TODO
}

#[tokio::test]
async fn drains_acknowledgements_at_shutdown() {
    // 1. Send N events (if running against a pre-populated kafka topic, use send_count=0 and expect_count=expected number of messages; otherwise just set send_count)
    let send_count: usize = std::env::var("KAFKA_SEND_COUNT")
        .unwrap_or_else(|_| "125000".into())
        .parse()
        .expect("Number of messages to send to kafka.");
    let expect_count: usize = std::env::var("KAFKA_EXPECT_COUNT")
        .unwrap_or_else(|_| format!("{send_count}"))
        .parse()
        .expect("Number of messages to expect consumers to process.");
    let delay_ms: u64 = std::env::var("KAFKA_SHUTDOWN_DELAY")
        .unwrap_or_else(|_| "2000".into())
        .parse()
        .expect("Number of milliseconds before shutting down first consumer.");

    let (topic, group_id, _) = send_to_test_topic(1, send_count).await;

    // 2. Run the kafka source to read some of the events
    // 3. Send a shutdown signal (at some point before all events are read)
    let mut opts = HashMap::new();
    // Set options to get partition EOF notifications, and fetch data in small/configurable size chunks
    opts.insert("enable.partition.eof".into(), "true".into());
    opts.insert("fetch.message.max.bytes".into(), kafka_max_bytes());
    let events1 = {
        let config = make_config(&topic, &group_id, LogNamespace::Legacy, Some(opts.clone()));
        let (tx, rx) = SourceSender::new_test_errors(|_| false);
        let (trigger_shutdown, shutdown_done) =
            spawn_kafka(tx, config, true, false, LogNamespace::Legacy);
        let (events, _) = tokio::join!(rx.collect::<Vec<Event>>(), async move {
            sleep(Duration::from_millis(delay_ms)).await;
            drop(trigger_shutdown);
        });
        shutdown_done.await;
        events
    };

    debug!("Consumer group.id: {}", &group_id);
    debug!(
        "First consumer read {} of {} messages.",
        events1.len(),
        expect_count
    );

    // 4. Run the kafka source again to finish reading the events
    let events2 = {
        let config = make_config(&topic, &group_id, LogNamespace::Legacy, Some(opts));
        let (tx, rx) = SourceSender::new_test_errors(|_| false);
        let (trigger_shutdown, shutdown_done) =
            spawn_kafka(tx, config, true, true, LogNamespace::Legacy);
        let events = rx.collect::<Vec<Event>>().await;
        drop(trigger_shutdown);
        shutdown_done.await;
        events
    };

    debug!(
        "Second consumer read {} of {} messages.",
        events2.len(),
        expect_count
    );

    // 5. Total number of events processed should equal the number sent
    let total = events1.len() + events2.len();
    assert_ne!(
        events1.len(),
        0,
        "First batch of events should be non-zero (increase KAFKA_SHUTDOWN_DELAY?)"
    );
    assert_ne!(
        events2.len(),
        0,
        "Second batch of events should be non-zero (decrease KAFKA_SHUTDOWN_DELAY or increase KAFKA_SEND_COUNT?) "
    );
    assert_eq!(total, expect_count);
}

async fn consume_with_rebalance(rebalance_strategy: String) {
    // 1. Send N events (if running against a pre-populated kafka topic, use send_count=0 and expect_count=expected number of messages; otherwise just set send_count)
    // A larger backlog gives the later consumers a bigger margin against being starved
    // (see the `events3` assertion below) as CI runners get faster at draining the topic.
    let send_count: usize = std::env::var("KAFKA_SEND_COUNT")
        .unwrap_or_else(|_| "500000".into())
        .parse()
        .expect("Number of messages to send to kafka.");
    let expect_count: usize = std::env::var("KAFKA_EXPECT_COUNT")
        .unwrap_or_else(|_| format!("{send_count}"))
        .parse()
        .expect("Number of messages to expect consumers to process.");
    let delay_ms: u64 = std::env::var("KAFKA_CONSUMER_DELAY")
        .unwrap_or_else(|_| "2000".into())
        .parse()
        .expect("Number of milliseconds before shutting down first consumer.");

    let (topic, group_id, _) = send_to_test_topic(6, send_count).await;
    debug!("Topic: {}", &topic);
    debug!("Consumer group.id: {}", &group_id);

    // 2. Run the kafka source to read some of the events
    // 3. Start 2nd & 3rd consumers using the same group.id, triggering rebalance events
    let mut kafka_options = HashMap::new();
    kafka_options.insert("enable.partition.eof".into(), "true".into());
    kafka_options.insert("fetch.message.max.bytes".into(), kafka_max_bytes());
    kafka_options.insert("partition.assignment.strategy".into(), rebalance_strategy);
    let config1 = make_config(
        &topic,
        &group_id,
        LogNamespace::Legacy,
        Some(kafka_options.clone()),
    );
    let config2 = config1.clone();
    let config3 = config1.clone();
    let config4 = config1.clone();

    let (events1, events2, events3) = tokio::join!(
        async move {
            let (tx, rx) = SourceSender::new_test_errors(|_| false);
            let (_trigger_shutdown, _shutdown_done) =
                spawn_kafka(tx, config1, true, true, LogNamespace::Legacy);

            rx.collect::<Vec<Event>>().await
        },
        async move {
            sleep(Duration::from_millis(delay_ms)).await;
            let (tx, rx) = SourceSender::new_test_errors(|_| false);
            let (_trigger_shutdown, _shutdown_done) =
                spawn_kafka(tx, config2, true, true, LogNamespace::Legacy);

            rx.collect::<Vec<Event>>().await
        },
        async move {
            sleep(Duration::from_millis(delay_ms * 2)).await;
            let (tx, rx) = SourceSender::new_test_errors(|_| false);
            let (_trigger_shutdown, _shutdown_done) =
                spawn_kafka(tx, config3, true, true, LogNamespace::Legacy);

            rx.collect::<Vec<Event>>().await
        }
    );

    let unconsumed = async move {
        let (tx, rx) = SourceSender::new_test_errors(|_| false);
        let (_trigger_shutdown, _shutdown_done) =
            spawn_kafka(tx, config4, true, true, LogNamespace::Legacy);

        rx.collect::<Vec<Event>>().await
    }
    .await;

    debug!(
        "First consumer read {} of {} messages.",
        events1.len(),
        expect_count
    );

    debug!(
        "Second consumer read {} of {} messages.",
        events2.len(),
        expect_count
    );
    debug!(
        "Third consumer read {} of {} messages.",
        events3.len(),
        expect_count
    );

    // 5. Total number of events processed should equal the number sent
    let total = events1.len() + events2.len() + events3.len();
    assert_ne!(
        events1.len(),
        0,
        "First batch of events should be non-zero (increase delay?)"
    );
    assert_ne!(
        events2.len(),
        0,
        "Second batch of events should be non-zero (decrease delay or increase KAFKA_SEND_COUNT?) "
    );
    assert_ne!(
        events3.len(),
        0,
        "Third batch of events should be non-zero (decrease delay or increase KAFKA_SEND_COUNT?) "
    );
    assert_eq!(
        unconsumed.len(),
        0,
        "The first set of consumers should consume and ack all messages."
    );
    // Kafka only guarantees at-least-once delivery: a partition revoked mid-rebalance
    // can be re-read by the consumer it's reassigned to before the prior consumer's
    // offset commit lands, so `total` may legitimately exceed `expect_count` if some
    // messages were delivered more than once.
    assert!(
        total >= expect_count,
        "Consumers should not lose any messages: got {total}, expected at least {expect_count}"
    );
    // Duplicates alone could mask a real drop behind an inflated `total`, so also check
    // that every message was seen by at least one consumer. When we produced the messages
    // ourselves via `send_events` (their `message_key` is `"{KEY} {index}"`), we can check
    // this by index. A pre-populated topic (`send_count == 0`, see above) can contain
    // arbitrary records, so instead we check the number of distinct Kafka
    // `(partition, offset)` pairs seen: since each source record has a unique offset,
    // duplicates can't inflate that count the way they inflate `total`, so it must equal
    // `expect_count` exactly for no message to have been lost.
    if send_count > 0 {
        let received: HashSet<usize> = message_indices(&events1)
            .into_iter()
            .chain(message_indices(&events2))
            .chain(message_indices(&events3))
            .collect();
        let missing: Vec<usize> = (0..expect_count)
            .filter(|i| !received.contains(i))
            .collect();
        assert!(
            missing.is_empty(),
            "Consumers should not lose any messages: got {total} events covering {}/{expect_count} indices, missing: {missing:?}",
            received.len(),
        );
    } else {
        let received: HashSet<(i64, i64)> = message_offsets(&events1)
            .into_iter()
            .chain(message_offsets(&events2))
            .chain(message_offsets(&events3))
            .collect();
        assert_eq!(
            received.len(),
            expect_count,
            "Consumers should not lose any messages: got {total} events covering only {}/{expect_count} unique offsets",
            received.len(),
        );
    }
}

#[tokio::test]
async fn drains_acknowledgements_during_rebalance_default_assignments() {
    // the default, eager rebalance strategies generally result in more revocations
    consume_with_rebalance("range,roundrobin".into()).await;
}
#[tokio::test]
async fn drains_acknowledgements_during_rebalance_sticky_assignments() {
    // Cooperative rebalance strategies generally result in fewer revokes,
    // as only reassigned partitions are revoked
    consume_with_rebalance("cooperative-sticky".into()).await;
}

fn map_logs(events: EventArray) -> impl Iterator<Item = String> {
    events.into_events().map(|event| {
        let log = event.into_log();
        format!(
            "{} {} {} {}",
            log["message"].to_string_lossy(),
            log["topic"].to_string_lossy(),
            log["partition"].to_string_lossy(),
            log["offset"].to_string_lossy(),
        )
    })
}
