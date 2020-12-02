use crate::{
    buffers::Acker,
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Value},
    kafka::{KafkaAuthConfig, KafkaCompression},
    serde::to_string,
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfigWithDefault, EncodingConfiguration},
        BatchConfig,
    },
    template::{Template, TemplateError},
};
use futures::{
    channel::oneshot::Canceled, future::BoxFuture, ready, stream::FuturesUnordered, FutureExt,
    Sink, StreamExt, TryFutureExt,
};
use rdkafka::{
    consumer::{BaseConsumer, Consumer},
    error::{KafkaError, RDKafkaError},
    producer::{DeliveryFuture, FutureProducer, FutureRecord},
    ClientConfig,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{sync::Notify, time::Duration};

// Maximum number of futures blocked by [send_result](https://docs.rs/rdkafka/0.24.0/rdkafka/producer/future_producer/struct.FutureProducer.html#method.send_result)
const SEND_RESULT_LIMIT: usize = 5;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("creating kafka producer failed: {}", source))]
    KafkaCreateFailed { source: KafkaError },
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateError },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KafkaSinkConfig {
    bootstrap_servers: String,
    topic: String,
    key_field: Option<String>,
    encoding: EncodingConfigWithDefault<Encoding>,
    /// These batching options will **not** override librdkafka_options values.
    #[serde(default)]
    batch: BatchConfig,
    #[serde(default)]
    compression: KafkaCompression,
    #[serde(flatten)]
    auth: KafkaAuthConfig,
    #[serde(default = "default_socket_timeout_ms")]
    socket_timeout_ms: u64,
    #[serde(default = "default_message_timeout_ms")]
    message_timeout_ms: u64,
    #[serde(default)]
    librdkafka_options: HashMap<String, String>,
}

fn default_socket_timeout_ms() -> u64 {
    60000 // default in librdkafka
}

fn default_message_timeout_ms() -> u64 {
    300000 // default in librdkafka
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Json,
}

pub struct KafkaSink {
    producer: Arc<FutureProducer>,
    topic: Template,
    key_field: Option<String>,
    encoding: EncodingConfig<Encoding>,
    flush_signal: Arc<Notify>,
    delivery_fut: FuturesUnordered<BoxFuture<'static, (usize, Result<DeliveryFuture, KafkaError>)>>,
    in_flight: FuturesUnordered<
        BoxFuture<'static, (usize, Result<Result<(i32, i64), KafkaError>, Canceled>)>,
    >,

    acker: Acker,
    seq_head: usize,
    seq_tail: usize,
    pending_acks: HashSet<usize>,
}

inventory::submit! {
    SinkDescription::new::<KafkaSinkConfig>("kafka")
}

impl GenerateConfig for KafkaSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"bootstrap_servers = "10.14.22.123:9092,10.14.23.332:9092"
            key_field = "user_id"
            topic = "topic-1234"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kafka")]
impl SinkConfig for KafkaSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = KafkaSink::new(self.clone(), cx.acker())?;
        let hc = healthcheck(self.clone()).boxed();
        Ok((super::VectorSink::Sink(Box::new(sink)), hc))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "kafka"
    }
}

impl KafkaSinkConfig {
    fn to_rdkafka(&self) -> crate::Result<ClientConfig> {
        let mut client_config = ClientConfig::new();
        client_config
            .set("bootstrap.servers", &self.bootstrap_servers)
            .set("compression.codec", &to_string(self.compression))
            .set("socket.timeout.ms", &self.socket_timeout_ms.to_string())
            .set("message.timeout.ms", &self.message_timeout_ms.to_string());

        self.auth.apply(&mut client_config)?;

        if let Some(queue_buffering_max_ms) = self.batch.timeout_secs {
            // Delay in milliseconds to wait for messages in the producer queue to accumulate before
            // constructing message batches (MessageSets) to transmit to brokers. A higher value
            // allows larger and more effective (less overhead, improved compression) batches of
            // messages to accumulate at the expense of increased message delivery latency.
            // Type: float
            let key = "queue.buffering.max.ms";
            if let Some(val) = self.librdkafka_options.get(key) {
                return Err(format!("Batching setting `batch.timeout_secs` sets `librdkafka_options.{}={}`.\
                                    The config already sets this as `librdkafka_options.queue.buffering.max.ms={}`.\
                                    Please delete one.", key, queue_buffering_max_ms, val).into());
            }
            client_config.set(key, &(queue_buffering_max_ms * 1000).to_string());
        }
        if let Some(batch_num_messages) = self.batch.max_events {
            // Maximum number of messages batched in one MessageSet. The total MessageSet size is
            // also limited by batch.size and message.max.bytes.
            // Type: integer
            let key = "batch.num.messages";
            if let Some(val) = self.librdkafka_options.get(key) {
                return Err(format!("Batching setting `batch.max_events` sets `librdkafka_options.{}={}`.\
                                    The config already sets this as `librdkafka_options.batch.num.messages={}`.\
                                    Please delete one.", key, batch_num_messages, val).into());
            }
            client_config.set(key, &batch_num_messages.to_string());
        }
        if let Some(batch_size) = self.batch.max_bytes {
            // Maximum size (in bytes) of all messages batched in one MessageSet, including protocol
            // framing overhead. This limit is applied after the first message has been added to the
            // batch, regardless of the first message's size, this is to ensure that messages that
            // exceed batch.size are produced. The total MessageSet size is also limited by
            // batch.num.messages and message.max.bytes.
            // Type: integer
            let key = "batch.size";
            if let Some(val) = self.librdkafka_options.get(key) {
                return Err(format!("Batching setting `batch.max_bytes` sets `librdkafka_options.{}={}`.\
                                    The config already sets this as `librdkafka_options.batch.size={}`.\
                                    Please delete one.", key, batch_size, val).into());
            }
            client_config.set(key, &batch_size.to_string());
        }

        for (key, value) in self.librdkafka_options.iter() {
            client_config.set(key.as_str(), value.as_str());
        }

        Ok(client_config)
    }
}

impl KafkaSink {
    fn new(config: KafkaSinkConfig, acker: Acker) -> crate::Result<Self> {
        let producer = config.to_rdkafka()?.create().context(KafkaCreateFailed)?;
        Ok(KafkaSink {
            producer: Arc::new(producer),
            topic: Template::try_from(config.topic).context(TopicTemplate)?,
            key_field: config.key_field,
            encoding: config.encoding.into(),
            flush_signal: Arc::new(Notify::new()),
            delivery_fut: FuturesUnordered::new(),
            in_flight: FuturesUnordered::new(),
            acker,
            seq_head: 0,
            seq_tail: 0,
            pending_acks: HashSet::new(),
        })
    }

    fn poll_delivery_fut(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        loop {
            match ready!(self.delivery_fut.poll_next_unpin(cx)) {
                Some((seqno, result)) => self.in_flight.push(Box::pin(async move {
                    let result = match result {
                        Ok(fut) => {
                            fut.map_ok(|result| result.map_err(|(error, _owned_message)| error))
                                .await
                        }
                        Err(error) => Ok(Err(error)),
                    };

                    (seqno, result)
                })),
                None => return Poll::Ready(()),
            }
        }
    }
}

impl Sink<Event> for KafkaSink {
    type Error = ();

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.poll_delivery_fut(cx) {
            Poll::Pending if self.delivery_fut.len() >= SEND_RESULT_LIMIT => Poll::Pending,
            _ => Poll::Ready(Ok(())),
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: Event) -> Result<(), Self::Error> {
        assert!(
            self.delivery_fut.len() < SEND_RESULT_LIMIT,
            "Expected `poll_ready` to be called first."
        );

        let topic = self.topic.render_string(&item).map_err(|missing_keys| {
            error!(message = "Missing keys for topic.", missing_keys = ?missing_keys);
        })?;
        let (key, body) = encode_event(item.clone(), &self.key_field, &self.encoding);

        let seqno = self.seq_head;
        self.seq_head += 1;

        let producer = Arc::clone(&self.producer);
        let flush_signal = Arc::clone(&self.flush_signal);
        self.delivery_fut.push(Box::pin(async move {
            let mut record = FutureRecord::to(&topic).key(&key).payload(&body[..]);
            if let Some(Value::Timestamp(timestamp)) =
                item.as_log().get(log_schema().timestamp_key())
            {
                record = record.timestamp(timestamp.timestamp_millis());
            }

            debug!(message = "Sending event.", count = 1);
            let result = loop {
                match producer.send_result(record) {
                    Ok(future) => break Ok(future),
                    // Try again if queue is full.
                    // See item 4 on GitHub: https://github.com/timberio/vector/pull/101#issue-257150924
                    // https://docs.rs/rdkafka/0.24.0/src/rdkafka/producer/future_producer.rs.html#296
                    Err((error, future_record))
                        if error == KafkaError::MessageProduction(RDKafkaError::QueueFull) =>
                    {
                        debug!(message = "The rdkafka queue full.", %error, %seqno, rate_limit_secs = 1);
                        record = future_record;
                        let _ = flush_signal.notified().await;
                    }
                    Err((error, _)) => break Err(error),
                }
            };

            (seqno, result)
        }));

        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = Pin::into_inner(self);

        while !this.delivery_fut.is_empty() || !this.in_flight.is_empty() {
            while let Poll::Ready(Some(item)) = this.in_flight.poll_next_unpin(cx) {
                this.flush_signal.notify();
                match item {
                    (seqno, Ok(result)) => {
                        match result {
                            Ok((partition, offset)) => {
                                trace!(message = "Produced message.", ?partition, ?offset)
                            }
                            Err(error) => error!(message = "Kafka error.", %error),
                        };

                        this.pending_acks.insert(seqno);

                        let mut num_to_ack = 0;
                        while this.pending_acks.remove(&this.seq_tail) {
                            num_to_ack += 1;
                            this.seq_tail += 1
                        }
                        this.acker.ack(num_to_ack);
                    }
                    (_seqno, Err(Canceled)) => {
                        error!(message = "Request canceled.");
                        return Poll::Ready(Err(()));
                    }
                }
            }

            ready!(this.poll_delivery_fut(cx));
        }

        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}

async fn healthcheck(config: KafkaSinkConfig) -> crate::Result<()> {
    let client = config.to_rdkafka().unwrap();
    let topic = match Template::try_from(config.topic)
        .context(TopicTemplate)?
        .render_string(&Event::from(""))
    {
        Ok(topic) => Some(topic),
        Err(missing_keys) => {
            warn!(
                message = "Could not generate topic for healthcheck.",
                ?missing_keys
            );
            None
        }
    };

    tokio::task::spawn_blocking(move || {
        let consumer: BaseConsumer = client.create().unwrap();
        let topic = topic.as_ref().map(|topic| &topic[..]);

        consumer
            .fetch_metadata(topic, Duration::from_secs(3))
            .map(|_| ())
    })
    .await??;

    Ok(())
}

fn encode_event(
    mut event: Event,
    key_field: &Option<String>,
    encoding: &EncodingConfig<Encoding>,
) -> (Vec<u8>, Vec<u8>) {
    let key = key_field
        .as_ref()
        .and_then(|f| event.as_log().get(f))
        .map(|v| v.as_bytes().to_vec())
        .unwrap_or_default();

    encoding.apply_rules(&mut event);

    let body = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&event.as_log()).unwrap(),
        Encoding::Text => event
            .as_log()
            .get(log_schema().message_key())
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    };

    (key, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KafkaSinkConfig>();
    }

    #[test]
    fn kafka_encode_event_text() {
        let key = "";
        let message = "hello world".to_string();
        let (key_bytes, bytes) = encode_event(
            message.clone().into(),
            &None,
            &EncodingConfig::from(Encoding::Text),
        );

        assert_eq!(&key_bytes[..], key.as_bytes());
        assert_eq!(&bytes[..], message.as_bytes());
    }

    #[test]
    fn kafka_encode_event_json() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        event.as_mut_log().insert("foo", "bar");

        let (key, bytes) = encode_event(
            event,
            &Some("key".into()),
            &EncodingConfig::from(Encoding::Json),
        );

        let map: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();

        assert_eq!(&key[..], b"value");
        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
        assert_eq!(map["foo"], "bar".to_string());
    }

    #[test]
    fn kafka_encode_event_apply_rules() {
        let mut event = Event::from("hello");
        event.as_mut_log().insert("key", "value");

        let (key, bytes) = encode_event(
            event,
            &Some("key".into()),
            &EncodingConfigWithDefault {
                codec: Encoding::Json,
                except_fields: Some(vec!["key".into()]),
                ..Default::default()
            }
            .into(),
        );

        let map: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();

        assert_eq!(&key[..], b"value");
        assert!(!map.contains_key("key"));
    }
}

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use super::*;
    use crate::{
        buffers::Acker,
        kafka::{KafkaAuthConfig, KafkaSaslConfig, KafkaTlsConfig},
        test_util::{random_lines_with_stream, random_string, wait_for},
        tls::TlsOptions,
    };
    use futures::StreamExt;
    use rdkafka::{
        consumer::{BaseConsumer, Consumer},
        Message, Offset, TopicPartitionList,
    };
    use std::{future::ready, thread, time::Duration};

    #[tokio::test]
    async fn healthcheck() {
        let topic = format!("test-{}", random_string(10));

        let config = KafkaSinkConfig {
            bootstrap_servers: "localhost:9091".into(),
            topic: topic.clone(),
            compression: KafkaCompression::None,
            encoding: EncodingConfigWithDefault::from(Encoding::Text),
            key_field: None,
            socket_timeout_ms: 60000,
            message_timeout_ms: 300000,
            ..Default::default()
        };

        super::healthcheck(config).await.unwrap();
    }

    #[tokio::test]
    async fn kafka_happy_path_plaintext() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::None).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_gzip() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Gzip).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_lz4() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Lz4).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_snappy() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Snappy).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_zstd() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Zstd).await;
    }

    fn kafka_batch_options_overrides(
        batch: BatchConfig,
        librdkafka_options: HashMap<String, String>,
    ) -> crate::Result<KafkaSink> {
        let topic = format!("test-{}", random_string(10));
        let config = KafkaSinkConfig {
            bootstrap_servers: "localhost:9091".to_string(),
            topic: format!("{}-%Y%m%d", topic),
            compression: KafkaCompression::None,
            encoding: EncodingConfigWithDefault::from(Encoding::Text),
            key_field: None,
            auth: KafkaAuthConfig {
                sasl: None,
                tls: None,
            },
            socket_timeout_ms: 60000,
            message_timeout_ms: 300000,
            batch,
            librdkafka_options,
        };
        let (acker, _ack_counter) = Acker::new_for_testing();
        KafkaSink::new(config, acker)
    }

    #[tokio::test]
    async fn kafka_batch_options_max_bytes_errors_on_double_set() {
        assert!(kafka_batch_options_overrides(
            BatchConfig {
                max_bytes: Some(1000),
                max_events: None,
                max_size: None,
                timeout_secs: None
            },
            indexmap::indexmap! {
                "batch.size".to_string() => 1.to_string(),
            }
            .into_iter()
            .collect()
        )
        .is_err())
    }

    #[tokio::test]
    async fn kafka_batch_options_max_events_errors_on_double_set() {
        assert!(kafka_batch_options_overrides(
            BatchConfig {
                max_bytes: None,
                max_events: Some(10),
                max_size: None,
                timeout_secs: None
            },
            indexmap::indexmap! {
                "batch.num.messages".to_string() => 1.to_string(),
            }
            .into_iter()
            .collect()
        )
        .is_err())
    }

    #[tokio::test]
    async fn kafka_batch_options_timeout_secs_errors_on_double_set() {
        assert!(kafka_batch_options_overrides(
            BatchConfig {
                max_bytes: None,
                max_events: None,
                max_size: None,
                timeout_secs: Some(10),
            },
            indexmap::indexmap! {
                "queue.buffering.max.ms".to_string() => 1.to_string(),
            }
            .into_iter()
            .collect()
        )
        .is_err())
    }

    #[tokio::test]
    async fn kafka_happy_path_tls() {
        kafka_happy_path(
            "localhost:9092",
            None,
            Some(KafkaTlsConfig {
                enabled: Some(true),
                options: TlsOptions::test_options(),
            }),
            KafkaCompression::None,
        )
        .await;
    }

    #[tokio::test]
    async fn kafka_happy_path_tls_with_key() {
        kafka_happy_path(
            "localhost:9092",
            None,
            Some(KafkaTlsConfig {
                enabled: Some(true),
                options: TlsOptions::test_options(),
            }),
            KafkaCompression::None,
        )
        .await;
    }

    #[tokio::test]
    async fn kafka_happy_path_sasl() {
        kafka_happy_path(
            "localhost:9093",
            Some(KafkaSaslConfig {
                enabled: Some(true),
                username: Some("admin".to_owned()),
                password: Some("admin".to_owned()),
                mechanism: Some("PLAIN".to_owned()),
            }),
            None,
            KafkaCompression::None,
        )
        .await;
    }

    async fn kafka_happy_path(
        server: &str,
        sasl: Option<KafkaSaslConfig>,
        tls: Option<KafkaTlsConfig>,
        compression: KafkaCompression,
    ) {
        let topic = format!("test-{}", random_string(10));

        let kafka_auth = KafkaAuthConfig { sasl, tls };
        let config = KafkaSinkConfig {
            bootstrap_servers: server.to_string(),
            topic: format!("{}-%Y%m%d", topic),
            compression,
            encoding: EncodingConfigWithDefault::from(Encoding::Text),
            key_field: None,
            auth: kafka_auth.clone(),
            socket_timeout_ms: 60000,
            message_timeout_ms: 300000,
            ..Default::default()
        };
        let topic = format!("{}-{}", topic, chrono::Utc::now().format("%Y%m%d"));
        let (acker, ack_counter) = Acker::new_for_testing();
        let sink = KafkaSink::new(config, acker).unwrap();

        let num_events = 1000;
        let (input, events) = random_lines_with_stream(100, num_events);
        events.map(Ok).forward(sink).await.unwrap();

        // read back everything from the beginning
        let mut client_config = rdkafka::ClientConfig::new();
        client_config.set("bootstrap.servers", server);
        client_config.set("group.id", &random_string(10));
        client_config.set("enable.partition.eof", "true");
        let _ = kafka_auth.apply(&mut client_config).unwrap();

        let mut tpl = TopicPartitionList::new();
        tpl.add_partition(&topic, 0).set_offset(Offset::Beginning);

        let consumer: BaseConsumer = client_config.create().unwrap();
        consumer.assign(&tpl).unwrap();

        // wait for messages to show up
        wait_for(|| {
            let (_low, high) = consumer
                .fetch_watermarks(&topic, 0, Duration::from_secs(3))
                .unwrap();
            ready(high > 0)
        })
        .await;

        // check we have the expected number of messages in the topic
        let (low, high) = consumer
            .fetch_watermarks(&topic, 0, Duration::from_secs(3))
            .unwrap();
        assert_eq!((0, num_events as i64), (low, high));

        // loop instead of iter so we can set a timeout
        let mut failures = 0;
        let mut out = Vec::new();
        while failures < 100 {
            match consumer.poll(Duration::from_secs(3)) {
                Some(Ok(msg)) => {
                    let s: &str = msg.payload_view().unwrap().unwrap();
                    out.push(s.to_owned());
                }
                None if out.len() >= input.len() => break,
                _ => {
                    failures += 1;
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }

        assert_eq!(out.len(), input.len());
        assert_eq!(out, input);

        assert_eq!(
            ack_counter.load(std::sync::atomic::Ordering::Relaxed),
            num_events
        );
    }
}
