use std::{
    collections::{BTreeMap, HashMap},
    io::Cursor,
    sync::Arc,
};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use futures::{Stream, StreamExt};
use once_cell::sync::OnceCell;
use rdkafka::{
    consumer::{Consumer, ConsumerContext, Rebalance, StreamConsumer},
    message::{BorrowedMessage, Headers, Message},
    ClientConfig, ClientContext, Statistics,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::FramedRead;
use vector_core::ByteSizeOf;

use super::util::finalizer::OrderedFinalizer;
use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{
        log_schema, AcknowledgementsConfig, Output, SourceConfig, SourceContext, SourceDescription,
    },
    event::{BatchNotifier, Event, Value},
    internal_events::{
        BytesReceived, KafkaEventsReceived, KafkaOffsetUpdateError, KafkaReadError,
        StreamClosedError,
    },
    kafka,
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    SourceSender,
};
use async_stream::stream;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not create Kafka consumer: {}", source))]
    KafkaCreateError { source: rdkafka::error::KafkaError },
    #[snafu(display("Could not subscribe to Kafka topics: {}", source))]
    KafkaSubscribeError { source: rdkafka::error::KafkaError },
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct KafkaSourceConfig {
    bootstrap_servers: String,
    topics: Vec<String>,
    group_id: String,
    #[serde(default = "default_auto_offset_reset")]
    auto_offset_reset: String,
    #[serde(default = "default_session_timeout_ms")]
    session_timeout_ms: u64,
    #[serde(default = "default_socket_timeout_ms")]
    socket_timeout_ms: u64,
    #[serde(default = "default_fetch_wait_max_ms")]
    fetch_wait_max_ms: u64,
    #[serde(default = "default_commit_interval_ms")]
    commit_interval_ms: u64,
    #[serde(default = "default_key_field")]
    key_field: String,
    #[serde(default = "default_topic_key")]
    topic_key: String,
    #[serde(default = "default_partition_key")]
    partition_key: String,
    #[serde(default = "default_offset_key")]
    offset_key: String,
    #[serde(default = "default_headers_key")]
    headers_key: String,
    librdkafka_options: Option<HashMap<String, String>>,
    #[serde(flatten)]
    auth: kafka::KafkaAuthConfig,
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: FramingConfig,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: DeserializerConfig,
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

const fn default_session_timeout_ms() -> u64 {
    10000 // default in librdkafka
}

const fn default_socket_timeout_ms() -> u64 {
    60000 // default in librdkafka
}

const fn default_fetch_wait_max_ms() -> u64 {
    100 // default in librdkafka
}

const fn default_commit_interval_ms() -> u64 {
    5000 // default in librdkafka
}

fn default_auto_offset_reset() -> String {
    "largest".into() // default in librdkafka
}

fn default_key_field() -> String {
    "message_key".into()
}

fn default_topic_key() -> String {
    "topic".into()
}

fn default_partition_key() -> String {
    "partition".into()
}

fn default_offset_key() -> String {
    "offset".into()
}

fn default_headers_key() -> String {
    "headers".into()
}

inventory::submit! {
    SourceDescription::new::<KafkaSourceConfig>("kafka")
}

impl_generate_config_from_default!(KafkaSourceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "kafka")]
impl SourceConfig for KafkaSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let consumer = create_consumer(self)?;
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build();
        let acknowledgements = cx.do_acknowledgements(&self.acknowledgements);

        Ok(Box::pin(kafka_source(
            self.clone(),
            consumer,
            decoder,
            cx.shutdown,
            cx.out,
            acknowledgements,
        )))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
    }

    fn source_type(&self) -> &'static str {
        "kafka"
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

async fn kafka_source(
    config: KafkaSourceConfig,
    consumer: StreamConsumer<CustomContext>,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
    acknowledgements: bool,
) -> Result<(), ()> {
    let consumer = Arc::new(consumer);
    let mut finalizer = acknowledgements.then(|| {
        let finalizer = {
            let consumer = Arc::clone(&consumer);
            Arc::new(OrderedFinalizer::new(
                shutdown.clone(),
                move |entry: FinalizerEntry| {
                    let consumer = Arc::clone(&consumer);
                    async move {
                        if let Err(error) =
                            consumer.store_offset(&entry.topic, entry.partition, entry.offset)
                        {
                            emit!(KafkaOffsetUpdateError { error });
                        }
                    }
                },
            ))
        };
        consumer
            .context()
            .finalizer
            .set(Arc::clone(&finalizer))
            .unwrap_or_else(|_| unreachable!());
        finalizer
    });
    let mut stream = consumer.stream().take_until(shutdown);
    let schema = log_schema();

    while let Some(message) = stream.next().await {
        match message {
            Err(error) => {
                emit!(KafkaReadError { error });
            }
            Ok(msg) => {
                emit!(BytesReceived {
                    byte_size: msg.payload_len(),
                    protocol: "tcp",
                });

                let payload = match msg.payload() {
                    None => continue, // skip messages with empty payload
                    Some(payload) => payload,
                };

                // Extract timestamp from kafka message
                let timestamp = msg
                    .timestamp()
                    .to_millis()
                    .and_then(|millis| Utc.timestamp_millis_opt(millis).latest())
                    .unwrap_or_else(Utc::now);

                let msg_key = msg
                    .key()
                    .map(|key| Value::from(Bytes::from(key.to_owned())))
                    .unwrap_or(Value::Null);

                let mut headers_map = BTreeMap::new();
                if let Some(headers) = msg.headers() {
                    for header in headers.iter() {
                        if let Some(value) = header.value {
                            headers_map.insert(
                                header.key.to_string(),
                                Value::from(Bytes::from(value.to_owned())),
                            );
                        }
                    }
                }

                let msg_topic = Bytes::copy_from_slice(msg.topic().as_bytes());
                let msg_partition = msg.partition();
                let msg_offset = msg.offset();

                let key_field = config.key_field.as_str();
                let topic_key = config.topic_key.as_str();
                let partition_key = config.partition_key.as_str();
                let offset_key = config.offset_key.as_str();
                let headers_key = config.headers_key.as_str();

                let payload = Cursor::new(Bytes::copy_from_slice(payload));

                let mut stream = FramedRead::new(payload, decoder.clone());
                let (count, _) = stream.size_hint();
                let mut stream = stream! {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok((events, _byte_size)) => {
                                emit!(KafkaEventsReceived {
                                    count: events.len(),
                                    byte_size: events.size_of(),
                                });
                                for mut event in events {
                                    if let Event::Log(ref mut log) = event {
                                        log.insert(schema.source_type_key(), Bytes::from("kafka"));
                                        log.insert(schema.timestamp_key(), timestamp);
                                        log.insert(key_field, msg_key.clone());
                                        log.insert(topic_key, Value::from(msg_topic.clone()));
                                        log.insert(partition_key, Value::from(msg_partition));
                                        log.insert(offset_key, Value::from(msg_offset));
                                        log.insert(headers_key, Value::from(headers_map.clone()));
                                    }

                                    yield event;
                                }
                            },
                            Err(error) => {
                                // Error is logged by `codecs::Decoder`, no further handling
                                // is needed here.
                                if !error.can_continue() {
                                    break;
                                }
                            }
                        }
                    }
                }
                .boxed();

                match &mut finalizer {
                    Some(finalizer) => {
                        let (batch, receiver) = BatchNotifier::new_with_receiver();
                        let mut stream = stream.map(|event| event.with_batch_notifier(&batch));
                        match out.send_event_stream(&mut stream).await {
                            Err(error) => {
                                emit!(StreamClosedError { error, count });
                            }
                            Ok(_) => {
                                // Drop stream to avoid borrowing `msg`: "[...] borrow might be used
                                // here, when `stream` is dropped and runs the destructor [...]".
                                drop(stream);
                                finalizer.add(msg.into(), receiver);
                            }
                        }
                    }
                    None => match out.send_event_stream(&mut stream).await {
                        Err(error) => {
                            emit!(StreamClosedError { error, count });
                        }
                        Ok(_) => {
                            if let Err(error) =
                                consumer.store_offset(msg.topic(), msg.partition(), msg.offset())
                            {
                                emit!(KafkaOffsetUpdateError { error });
                            }
                        }
                    },
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
struct FinalizerEntry {
    topic: String,
    partition: i32,
    offset: i64,
}

impl<'a> From<BorrowedMessage<'a>> for FinalizerEntry {
    fn from(msg: BorrowedMessage<'a>) -> Self {
        Self {
            topic: msg.topic().into(),
            partition: msg.partition(),
            offset: msg.offset(),
        }
    }
}

fn create_consumer(config: &KafkaSourceConfig) -> crate::Result<StreamConsumer<CustomContext>> {
    let mut client_config = ClientConfig::new();
    client_config
        .set("group.id", &config.group_id)
        .set("bootstrap.servers", &config.bootstrap_servers)
        .set("auto.offset.reset", &config.auto_offset_reset)
        .set("session.timeout.ms", &config.session_timeout_ms.to_string())
        .set("socket.timeout.ms", &config.socket_timeout_ms.to_string())
        .set("fetch.wait.max.ms", &config.fetch_wait_max_ms.to_string())
        .set("enable.partition.eof", "false")
        .set("enable.auto.commit", "true")
        .set(
            "auto.commit.interval.ms",
            &config.commit_interval_ms.to_string(),
        )
        .set("enable.auto.offset.store", "false")
        .set("statistics.interval.ms", "1000")
        .set("client.id", "vector");

    config.auth.apply(&mut client_config)?;

    if let Some(librdkafka_options) = &config.librdkafka_options {
        for (key, value) in librdkafka_options {
            client_config.set(key.as_str(), value.as_str());
        }
    }

    let consumer = client_config
        .create_with_context::<_, StreamConsumer<_>>(CustomContext::default())
        .context(KafkaCreateSnafu)?;
    let topics: Vec<&str> = config.topics.iter().map(|s| s.as_str()).collect();
    consumer.subscribe(&topics).context(KafkaSubscribeSnafu)?;

    Ok(consumer)
}

#[derive(Default)]
struct CustomContext {
    stats: kafka::KafkaStatisticsContext,
    finalizer: OnceCell<Arc<OrderedFinalizer<FinalizerEntry>>>,
}

impl ClientContext for CustomContext {
    fn stats(&self, statistics: Statistics) {
        self.stats.stats(statistics)
    }
}

impl ConsumerContext for CustomContext {
    fn post_rebalance(&self, rebalance: &Rebalance) {
        if matches!(rebalance, Rebalance::Revoke(_)) {
            if let Some(finalizer) = self.finalizer.get() {
                finalizer.flush();
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    pub fn kafka_host() -> String {
        std::env::var("KAFKA_HOST").unwrap_or_else(|_| "localhost".into())
    }

    pub fn kafka_address(port: u16) -> String {
        format!("{}:{}", kafka_host(), port)
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KafkaSourceConfig>();
    }

    pub(super) fn make_config(topic: &str, group: &str) -> KafkaSourceConfig {
        KafkaSourceConfig {
            bootstrap_servers: kafka_address(9091),
            topics: vec![topic.into()],
            group_id: group.into(),
            auto_offset_reset: "beginning".into(),
            session_timeout_ms: 6000,
            commit_interval_ms: 1,
            key_field: "message_key".to_string(),
            topic_key: "topic".to_string(),
            partition_key: "partition".to_string(),
            offset_key: "offset".to_string(),
            headers_key: "headers".to_string(),
            socket_timeout_ms: 60000,
            fetch_wait_max_ms: 100,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn consumer_create_ok() {
        let config = make_config("topic", "group");
        assert!(create_consumer(&config).is_ok());
    }

    #[tokio::test]
    async fn consumer_create_incorrect_auto_offset_reset() {
        let config = KafkaSourceConfig {
            auto_offset_reset: "incorrect-auto-offset-reset".to_string(),
            ..make_config("topic", "group")
        };
        assert!(create_consumer(&config).is_err());
    }
}

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use std::time::Duration;

    use chrono::{DateTime, SubsecRound, Utc};
    use futures::Stream;
    use rdkafka::{
        admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
        client::DefaultClientContext,
        config::{ClientConfig, FromClientConfig},
        consumer::BaseConsumer,
        message::{Header, OwnedHeaders},
        producer::{FutureProducer, FutureRecord},
        util::Timeout,
        Offset, TopicPartitionList,
    };
    use stream_cancel::{Trigger, Tripwire};
    use tokio::time::sleep;
    use vector_buffers::topology::channel::BufferReceiver;
    use vector_core::event::EventStatus;

    use super::{test::*, *};
    use crate::{
        event::{EventArray, EventContainer},
        shutdown::ShutdownSignal,
        test_util::{collect_n, random_string},
        SourceSender,
    };

    const KEY: &str = "my key";
    const TEXT: &str = "my message";
    const HEADER_KEY: &str = "my header";
    const HEADER_VALUE: &str = "my header value";

    fn client_config<T: FromClientConfig>(group: Option<&str>) -> T {
        let mut client = ClientConfig::new();
        client.set("bootstrap.servers", kafka_address(9091));
        client.set("produce.offset.report", "true");
        client.set("message.timeout.ms", "5000");
        client.set("auto.commit.interval.ms", "1");
        if let Some(group) = group {
            client.set("group.id", group);
        }
        client.create().expect("Producer creation error")
    }

    async fn send_events(topic: String, count: usize) -> DateTime<Utc> {
        let now = Utc::now();
        let timestamp = now.timestamp_millis();

        let producer: FutureProducer = client_config(None);

        for i in 0..count {
            let text = format!("{} {:03}", TEXT, i);
            let key = format!("{} {}", KEY, i);
            let record = FutureRecord::to(&topic)
                .payload(&text)
                .key(&key)
                .timestamp(timestamp)
                .headers(OwnedHeaders::new().insert(Header {
                    key: HEADER_KEY,
                    value: Some(HEADER_VALUE),
                }));

            if let Err(error) = producer.send(record, Timeout::Never).await {
                panic!("Cannot send event to Kafka: {:?}", error);
            }
        }

        now
    }

    #[tokio::test]
    async fn consumes_event_with_acknowledgements() {
        consume_event(true).await;
    }

    #[tokio::test]
    async fn consumes_event_without_acknowledgements() {
        consume_event(false).await;
    }

    async fn consume_event(acknowledgements: bool) {
        let (topic, group_id, config) = make_rand_config();

        let now = send_events(topic.clone(), 10).await;

        let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);
        let (trigger_shutdown, shutdown_done) = spawn_kafka(tx, config, acknowledgements);
        let events = collect_n(rx, 10).await;
        drop(trigger_shutdown);
        shutdown_done.await;

        let offset = fetch_tpl_offset(&group_id, &topic, 0);
        assert_eq!(offset, Offset::from_raw(10));

        assert_eq!(events.len(), 10);
        for (i, event) in events.into_iter().enumerate() {
            assert_eq!(
                event.as_log()[log_schema().message_key()],
                format!("{} {:03}", TEXT, i).into()
            );
            assert_eq!(
                event.as_log()["message_key"],
                format!("{} {}", KEY, i).into()
            );
            assert_eq!(
                event.as_log()[log_schema().source_type_key()],
                "kafka".into()
            );
            assert_eq!(
                event.as_log()[log_schema().timestamp_key()],
                now.trunc_subsecs(3).into()
            );
            assert_eq!(event.as_log()["topic"], topic.clone().into());
            assert!(event.as_log().contains("partition"));
            assert!(event.as_log().contains("offset"));
            let mut expected_headers = BTreeMap::new();
            expected_headers.insert(HEADER_KEY.to_string(), Value::from(HEADER_VALUE));
            assert_eq!(event.as_log()["headers"], Value::from(expected_headers));
        }
    }

    fn make_rand_config() -> (String, String, KafkaSourceConfig) {
        let topic = format!("test-topic-{}", random_string(10));
        let group_id = format!("test-group-{}", random_string(10));
        let config = make_config(&topic, &group_id);
        (topic, group_id, config)
    }

    fn delay_pipeline(
        id: usize,
        delay: Duration,
        status: EventStatus,
    ) -> (SourceSender, impl Stream<Item = EventArray> + Unpin) {
        let (pipe, recv) = SourceSender::new_with_buffer(100);
        let recv = BufferReceiver::new(recv.into()).into_stream();
        let recv = recv.then(move |mut events| async move {
            events.for_each_log(|log| {
                log.insert("pipeline_id", id.to_string());
            });
            sleep(delay).await;
            events.for_each_event(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            events
        });
        (pipe, Box::pin(recv))
    }

    fn spawn_kafka(
        tx: SourceSender,
        config: KafkaSourceConfig,
        acknowledgements: bool,
    ) -> (Trigger, Tripwire) {
        let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();
        let consumer = create_consumer(&config).unwrap();
        tokio::spawn(kafka_source(
            config,
            consumer,
            Decoder::default(),
            shutdown,
            tx,
            acknowledgements,
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

    async fn create_topic(group_id: &str, topic: &str, partitions: i32) {
        let client: AdminClient<DefaultClientContext> = client_config(Some(group_id));
        for result in client
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
            .expect("create_topics failed")
        {
            result.expect("Creating a topic failed");
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
        create_topic(&group_id, &topic, 2).await;

        let _send_start = send_events(topic.clone(), NEVENTS).await;

        let (tx, rx1) = delay_pipeline(1, Duration::from_millis(200), EventStatus::Delivered);
        let (trigger_shutdown1, shutdown_done1) = spawn_kafka(tx, config.clone(), true);
        let events1 = tokio::spawn(collect_n(rx1, NEVENTS));

        sleep(Duration::from_secs(1)).await;

        let (tx, rx2) = delay_pipeline(2, Duration::from_millis(DELAY), EventStatus::Delivered);
        let (trigger_shutdown2, shutdown_done2) = spawn_kafka(tx, config, true);
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
            o => panic!("Invalid offset for partition 0 {:?}", o),
        }

        match fetch_tpl_offset(&group_id, &topic, 1) {
            Offset::Offset(offset) => {
                assert!((offset as isize - events2.len() as isize).abs() <= 1)
            }
            o => panic!("Invalid offset for partition 0 {:?}", o),
        }

        let mut all_events = events1
            .into_iter()
            .chain(events2.into_iter())
            .map(map_logs)
            .flatten()
            .collect::<Vec<String>>();
        all_events.sort();

        // Assert they are all in sequential order and no dupes, TODO
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
}
