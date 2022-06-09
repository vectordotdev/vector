use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Cursor,
    sync::Arc,
};

use async_stream::stream;
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use futures::{Stream, StreamExt};
use rdkafka::{
    config::ClientConfig,
    consumer::{Consumer, StreamConsumer},
    message::{BorrowedMessage, Headers, Message},
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::FramedRead;
use vector_core::{finalizer::OrderedFinalizer, ByteSizeOf};

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{
        log_schema, AcknowledgementsConfig, LogSchema, Output, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::{BatchNotifier, BatchStatus, Event, Value},
    internal_events::{
        KafkaBytesReceived, KafkaEventsReceived, KafkaNegativeAcknowledgmentError,
        KafkaOffsetUpdateError, KafkaReadError, StreamClosedError,
    },
    kafka::{KafkaAuthConfig, KafkaStatisticsContext},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    SourceSender,
};

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
    auth: KafkaAuthConfig,
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
    consumer: StreamConsumer<KafkaStatisticsContext>,
    decoder: Decoder,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
    acknowledgements: bool,
) -> Result<(), ()> {
    let consumer = Arc::new(consumer);
    let (finalizer, mut ack_stream) =
        OrderedFinalizer::<FinalizerEntry>::maybe_new(acknowledgements, shutdown.clone());
    let mut stream = consumer.stream();
    let keys = Keys::from(log_schema(), &config);

    let mut topics = Topics::new(&config);

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            entry = ack_stream.next() => if let Some((status, entry)) = entry {
                handle_ack(&mut topics, status, entry, &consumer);
            },
            message = stream.next() => match message {
                None => break,  // WHY?
                Some(Err(error)) => emit!(KafkaReadError { error }),
                Some(Ok(msg)) => {
                    emit!(KafkaBytesReceived {
                        byte_size: msg.payload_len(),
                        protocol: "tcp",
                        topic: msg.topic(),
                        partition: msg.partition(),
                    });

                    parse_message(msg, &decoder, keys, &finalizer, &mut out, &consumer, &topics).await;
                }
            },
        }
    }

    Ok(())
}

struct Topics {
    subscribed: HashSet<String>,
    failed: HashSet<String>,
}

impl Topics {
    fn new(config: &KafkaSourceConfig) -> Self {
        Self {
            subscribed: config.topics.iter().cloned().collect(),
            failed: Default::default(),
        }
    }
}

fn handle_ack(
    topics: &mut Topics,
    status: BatchStatus,
    entry: FinalizerEntry,
    consumer: &StreamConsumer<KafkaStatisticsContext>,
) {
    if !topics.failed.contains(&entry.topic) {
        if status == BatchStatus::Delivered {
            if let Err(error) = consumer.store_offset(&entry.topic, entry.partition, entry.offset) {
                emit!(KafkaOffsetUpdateError { error });
            }
        } else {
            emit!(KafkaNegativeAcknowledgmentError {
                topic: &entry.topic,
                partition: entry.partition,
                offset: entry.offset,
            });
            // Try to unsubscribe from the named topic. Note that the
            // subscribed topics list could be missing the named topic
            // for two reasons:
            // 1. Multiple batches of events from the same topic could
            // be flight and all receive a negative acknowledgement,
            // in which case it will only be present for the first
            // response.
            // 2. The topic list may contain wildcards, in which case
            // there may not be an exact match for the topic name.
            if topics.subscribed.remove(&entry.topic) {
                let topics: Vec<&str> = topics.subscribed.iter().map(|s| s.as_str()).collect();
                // There is no direct way to unsubscribe from a named
                // topic, as the unsubscribe library function drops
                // all topics. The subscribe function, however,
                // replaces the list of subscriptions, from which we
                // have removed the topic above.  Ignore any errors,
                // as we drop output from the topic below anyways.
                let _ = consumer.subscribe(&topics);
            }
            // Don't update the offset after a failed ack
            topics.failed.insert(entry.topic);
        }
    }
}

async fn parse_message(
    msg: BorrowedMessage<'_>,
    decoder: &Decoder,
    keys: Keys<'_>,
    finalizer: &Option<OrderedFinalizer<FinalizerEntry>>,
    out: &mut SourceSender,
    consumer: &Arc<StreamConsumer<KafkaStatisticsContext>>,
    topics: &Topics,
) {
    if let Some((count, mut stream)) = parse_stream(&msg, decoder, keys, topics) {
        match finalizer {
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

// Turn the received message into a stream of parsed events.
fn parse_stream<'a>(
    msg: &BorrowedMessage<'a>,
    decoder: &Decoder,
    keys: Keys<'a>,
    topics: &Topics,
) -> Option<(usize, impl Stream<Item = Event> + 'a)> {
    if topics.failed.contains(msg.topic()) {
        return None;
    }

    let payload = msg.payload()?; // skip messages with empty payload

    let rmsg = ReceivedMessage::from(msg);

    let payload = Cursor::new(Bytes::copy_from_slice(payload));

    let mut stream = FramedRead::new(payload, decoder.clone());
    let (count, _) = stream.size_hint();
    let stream = stream! {
        while let Some(result) = stream.next().await {
            match result {
                Ok((events, _byte_size)) => {
                    emit!(KafkaEventsReceived {
                        count: events.len(),
                        byte_size: events.size_of(),
                        topic: &rmsg.topic,
                        partition: rmsg.partition,
                    });
                    for mut event in events {
                        rmsg.apply(&keys, &mut event);
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
    Some((count, stream))
}

#[derive(Clone, Copy)]
struct Keys<'a> {
    source_type: &'a str,
    timestamp: &'a str,
    key_field: &'a str,
    topic: &'a str,
    partition: &'a str,
    offset: &'a str,
    headers: &'a str,
}

impl<'a> Keys<'a> {
    fn from(schema: &'a LogSchema, config: &'a KafkaSourceConfig) -> Self {
        Self {
            source_type: schema.source_type_key(),
            timestamp: schema.timestamp_key(),
            key_field: config.key_field.as_str(),
            topic: config.topic_key.as_str(),
            partition: config.partition_key.as_str(),
            offset: config.offset_key.as_str(),
            headers: config.headers_key.as_str(),
        }
    }
}

struct ReceivedMessage {
    timestamp: DateTime<Utc>,
    key: Value,
    headers: BTreeMap<String, Value>,
    topic: String,
    partition: i32,
    offset: i64,
}

impl ReceivedMessage {
    fn from(msg: &BorrowedMessage<'_>) -> Self {
        // Extract timestamp from kafka message
        let timestamp = msg
            .timestamp()
            .to_millis()
            .and_then(|millis| Utc.timestamp_millis_opt(millis).latest())
            .unwrap_or_else(Utc::now);

        let key = msg
            .key()
            .map(|key| Value::from(Bytes::from(key.to_owned())))
            .unwrap_or(Value::Null);

        let mut headers_map = BTreeMap::new();
        if let Some(headers) = msg.headers() {
            // Using index-based for loop because rdkafka's `Headers` trait
            // does not provide Iterator-based API
            for i in 0..headers.count() {
                if let Some(header) = headers.get(i) {
                    headers_map.insert(
                        header.0.to_string(),
                        Bytes::from(header.1.to_owned()).into(),
                    );
                }
            }
        }

        Self {
            timestamp,
            key,
            headers: headers_map,
            topic: msg.topic().to_string(),
            partition: msg.partition(),
            offset: msg.offset(),
        }
    }

    fn apply(&self, keys: &Keys<'_>, event: &mut Event) {
        if let Event::Log(ref mut log) = event {
            log.insert(keys.source_type, Bytes::from("kafka"));
            log.insert(keys.timestamp, self.timestamp);
            log.insert(keys.key_field, self.key.clone());
            log.insert(keys.topic, Value::from(self.topic.clone()));
            log.insert(keys.partition, Value::from(self.partition));
            log.insert(keys.offset, Value::from(self.offset));
            log.insert(keys.headers, Value::from(self.headers.clone()));
        }
    }
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

fn create_consumer(
    config: &KafkaSourceConfig,
) -> crate::Result<StreamConsumer<KafkaStatisticsContext>> {
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
        .create_with_context::<_, StreamConsumer<_>>(KafkaStatisticsContext)
        .context(KafkaCreateSnafu)?;
    let topics: Vec<&str> = config.topics.iter().map(|s| s.as_str()).collect();
    consumer.subscribe(&topics).context(KafkaSubscribeSnafu)?;

    Ok(consumer)
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
            commit_interval_ms: 5000,
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

    use chrono::{SubsecRound, Utc};
    use rdkafka::{
        config::{ClientConfig, FromClientConfig},
        consumer::BaseConsumer,
        message::OwnedHeaders,
        producer::{FutureProducer, FutureRecord},
        util::Timeout,
        Offset, TopicPartitionList,
    };

    use super::{test::*, *};
    use crate::{
        shutdown::ShutdownSignal,
        test_util::{collect_n, components::assert_source_compliance, random_string},
        SourceSender,
    };

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

    async fn send_events(
        topic: &str,
        count: usize,
        key: &str,
        text: &str,
        timestamp: i64,
        header_key: &str,
        header_value: &str,
    ) {
        let producer: FutureProducer = client_config(None);

        for i in 0..count {
            let text = format!("{} {}", text, i);
            let record = FutureRecord::to(topic)
                .payload(&text)
                .key(key)
                .timestamp(timestamp)
                .headers(OwnedHeaders::new().add(header_key, header_value));

            if let Err(error) = producer.send(record, Timeout::Never).await {
                panic!("Cannot send event to Kafka: {:?}", error);
            }
        }
    }

    #[tokio::test]
    async fn consumes_event_with_acknowledgements() {
        send_receive(true, 10).await;
    }

    #[tokio::test]
    async fn consumes_event_without_acknowledgements() {
        send_receive(false, 10).await;
    }

    #[tokio::test]
    async fn handles_negative_acknowledgements() {
        send_receive(true, 2).await;
    }

    async fn send_receive(acknowledgements: bool, receive_count: usize) {
        const SEND_COUNT: usize = 10;

        let topic = format!("test-topic-{}", random_string(10));
        let group_id = format!("test-group-{}", random_string(10));
        let now = Utc::now();

        let config = make_config(&topic, &group_id);

        send_events(
            &topic,
            SEND_COUNT,
            "my key",
            "my message",
            now.timestamp_millis(),
            "my header",
            "my header value",
        )
        .await;

        let events = assert_source_compliance(&["protocol", "topic", "partition"], async move {
            let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();
            let (tx, rx) = SourceSender::new_test_error_after(receive_count);
            let consumer = create_consumer(&config).unwrap();
            tokio::spawn(kafka_source(
                config,
                consumer,
                crate::codecs::Decoder::default(),
                shutdown,
                tx,
                acknowledgements,
            ));
            let events = collect_n(rx, SEND_COUNT).await;
            // Yield to the finalization task to let it collect the
            // batch status receivers before signalling the shutdown.
            tokio::task::yield_now().await;
            drop(trigger_shutdown);
            shutdown_done.await;

            events
        })
        .await;

        let client: BaseConsumer = client_config(Some(&group_id));
        client.subscribe(&[&topic]).expect("Subscribing failed");

        let mut tpl = TopicPartitionList::new();
        tpl.add_partition(&topic, 0);
        let tpl = client
            .committed_offsets(tpl, Duration::from_secs(1))
            .expect("Getting committed offsets failed");
        assert_eq!(
            tpl.find_partition(&topic, 0)
                .expect("TPL is missing topic")
                .offset(),
            Offset::from_raw(receive_count as i64)
        );

        assert_eq!(events.len(), SEND_COUNT);
        for (i, event) in events.into_iter().enumerate() {
            assert_eq!(
                event.as_log()[log_schema().message_key()],
                format!("my message {}", i).into()
            );
            assert_eq!(event.as_log()["message_key"], "my key".into());
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
            expected_headers.insert("my header".to_string(), Value::from("my header value"));
            assert_eq!(event.as_log()["headers"], Value::from(expected_headers));
        }
    }
}
