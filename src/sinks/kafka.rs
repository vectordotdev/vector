use crate::{
    buffers::Acker,
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    internal_events::{KafkaHeaderExtractionFailed, TemplateRenderingFailed},
    kafka::{KafkaAuthConfig, KafkaCompression, KafkaStatisticsContext},
    serde::to_string,
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        BatchConfig,
    },
    template::{Template, TemplateParseError},
};
use futures::{
    channel::oneshot::Canceled, future::BoxFuture, ready, stream::FuturesUnordered, FutureExt,
    Sink, Stream, TryFutureExt,
};
use rdkafka::{
    consumer::{BaseConsumer, Consumer},
    error::{KafkaError, RDKafkaErrorCode},
    message::OwnedHeaders,
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
use tokio::time::{sleep, Duration};
use vector_core::event::{Event, EventMetadata, EventStatus, Value};

// Maximum number of futures blocked by [send_result](https://docs.rs/rdkafka/0.24.0/rdkafka/producer/future_producer/struct.FutureProducer.html#method.send_result)
const SEND_RESULT_LIMIT: usize = 5;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("creating kafka producer failed: {}", source))]
    KafkaCreateFailed { source: KafkaError },
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateParseError },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KafkaSinkConfig {
    bootstrap_servers: String,
    topic: String,
    key_field: Option<String>,
    encoding: EncodingConfig<Encoding>,
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
    headers_key: Option<String>,
}

const fn default_socket_timeout_ms() -> u64 {
    60000 // default in librdkafka
}

const fn default_message_timeout_ms() -> u64 {
    300000 // default in librdkafka
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

pub struct KafkaSink {
    producer: Arc<FutureProducer<KafkaStatisticsContext>>,
    topic: Template,
    key_field: Option<String>,
    encoding: EncodingConfig<Encoding>,
    delivery_fut: FuturesUnordered<
        BoxFuture<'static, (usize, Result<DeliveryFuture, KafkaError>, EventMetadata)>,
    >,
    in_flight: FuturesUnordered<
        BoxFuture<
            'static,
            (
                usize,
                Result<Result<(i32, i64), KafkaError>, Canceled>,
                EventMetadata,
            ),
        >,
    >,

    acker: Acker,
    seq_head: usize,
    seq_tail: usize,
    pending_acks: HashSet<usize>,
    headers_key: Option<String>,
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
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "kafka"
    }
}

/// Used to determine the options to set in configs, since both Kafka consumers and producers have
/// unique options, they use the same struct, and the error if given the wrong options.
#[derive(Debug, PartialOrd, PartialEq)]
enum KafkaRole {
    Consumer,
    Producer,
}

impl KafkaSinkConfig {
    fn to_rdkafka(&self, kafka_role: KafkaRole) -> crate::Result<ClientConfig> {
        let mut client_config = ClientConfig::new();
        client_config
            .set("bootstrap.servers", &self.bootstrap_servers)
            .set("compression.codec", &to_string(self.compression))
            .set("socket.timeout.ms", &self.socket_timeout_ms.to_string())
            .set("message.timeout.ms", &self.message_timeout_ms.to_string())
            .set("statistics.interval.ms", "1000");

        self.auth.apply(&mut client_config)?;

        // All batch options are producer only.
        if kafka_role == KafkaRole::Producer {
            if let Some(value) = self.batch.timeout_secs {
                // Delay in milliseconds to wait for messages in the producer queue to accumulate before
                // constructing message batches (MessageSets) to transmit to brokers. A higher value
                // allows larger and more effective (less overhead, improved compression) batches of
                // messages to accumulate at the expense of increased message delivery latency.
                // Type: float
                let key = "queue.buffering.max.ms";
                if let Some(val) = self.librdkafka_options.get(key) {
                    return Err(format!("Batching setting `batch.timeout_secs` sets `librdkafka_options.{}={}`.\
                                    The config already sets this as `librdkafka_options.queue.buffering.max.ms={}`.\
                                    Please delete one.", key, value, val).into());
                }
                debug!(
                    librdkafka_option = key,
                    batch_option = "timeout_secs",
                    value,
                    "Applying batch option as librdkafka option."
                );
                client_config.set(key, &(value * 1000).to_string());
            }
            if let Some(value) = self.batch.max_events {
                // Maximum number of messages batched in one MessageSet. The total MessageSet size is
                // also limited by batch.size and message.max.bytes.
                // Type: integer
                let key = "batch.num.messages";
                if let Some(val) = self.librdkafka_options.get(key) {
                    return Err(format!("Batching setting `batch.max_events` sets `librdkafka_options.{}={}`.\
                                    The config already sets this as `librdkafka_options.batch.num.messages={}`.\
                                    Please delete one.", key, value, val).into());
                }
                debug!(
                    librdkafka_option = key,
                    batch_option = "max_events",
                    value,
                    "Applying batch option as librdkafka option."
                );
                client_config.set(key, &value.to_string());
            }
            if let Some(value) = self.batch.max_bytes {
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
                                    Please delete one.", key, value, val).into());
                }
                debug!(
                    librdkafka_option = key,
                    batch_option = "max_bytes",
                    value,
                    "Applying batch option as librdkafka option."
                );
                client_config.set(key, &value.to_string());
            }
        }

        for (key, value) in self.librdkafka_options.iter() {
            debug!(option = %key, value = %value, "Setting librdkafka option.");
            client_config.set(key.as_str(), value.as_str());
        }

        Ok(client_config)
    }
}

impl KafkaSink {
    fn new(config: KafkaSinkConfig, acker: Acker) -> crate::Result<Self> {
        let producer_config = config.to_rdkafka(KafkaRole::Producer)?;
        let producer = producer_config
            .create_with_context(KafkaStatisticsContext)
            .context(KafkaCreateFailed)?;
        Ok(KafkaSink {
            producer: Arc::new(producer),
            topic: Template::try_from(config.topic).context(TopicTemplate)?,
            key_field: config.key_field,
            encoding: config.encoding,
            delivery_fut: FuturesUnordered::new(),
            in_flight: FuturesUnordered::new(),
            acker,
            seq_head: 0,
            seq_tail: 0,
            pending_acks: HashSet::new(),
            headers_key: config.headers_key,
        })
    }

    fn poll_delivery_fut(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        while !self.delivery_fut.is_empty() {
            let result = Pin::new(&mut self.delivery_fut).poll_next(cx);
            let (seqno, result, metadata) =
                ready!(result).expect("`delivery_fut` is endless stream");
            self.in_flight.push(Box::pin(async move {
                let result = match result {
                    Ok(fut) => {
                        fut.map_ok(|result| result.map_err(|(error, _owned_message)| error))
                            .await
                    }
                    Err(error) => Ok(Err(error)),
                };

                (seqno, result, metadata)
            }));
        }

        Poll::Ready(())
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

        let topic = self.topic.render_string(&item).map_err(|error| {
            emit!(&TemplateRenderingFailed {
                error,
                field: Some("topic"),
                drop_event: true,
            });
        })?;

        let timestamp_ms = match &item {
            Event::Log(log) => log
                .get(log_schema().timestamp_key())
                .and_then(|v| v.as_timestamp())
                .copied(),
            Event::Metric(metric) => metric.timestamp(),
        }
        .map(|ts| ts.timestamp_millis());

        let headers = self
            .headers_key
            .as_ref()
            .and_then(|headers_key| get_headers(&item, headers_key));

        let (key, body, metadata) = encode_event(item, &self.key_field, &self.encoding);

        let seqno = self.seq_head;
        self.seq_head += 1;

        let producer = Arc::clone(&self.producer);
        let has_key_field = self.key_field.is_some();

        self.delivery_fut.push(Box::pin(async move {
            let mut record = FutureRecord::to(&topic).payload(&body[..]);
            if has_key_field {
                record = record.key(&key);
            }
            if let Some(timestamp) = timestamp_ms {
                record = record.timestamp(timestamp);
            }
            if let Some(headers) = headers {
                record = record.headers(headers);
            }

            let result = loop {
                debug!(message = "Sending event.", count = 1);
                match producer.send_result(record) {
                    Ok(future) => break Ok(future),
                    // Try again if queue is full.
                    // See item 4 on GitHub: https://github.com/timberio/vector/pull/101#issue-257150924
                    // https://docs.rs/rdkafka/0.24.0/src/rdkafka/producer/future_producer.rs.html#296
                    Err((error, future_record))
                        if error == KafkaError::MessageProduction(RDKafkaErrorCode::QueueFull) =>
                    {
                        debug!(message = "The rdkafka queue is full.", %error, %seqno, internal_log_rate_secs = 1);
                        record = future_record;
                        sleep(Duration::from_millis(10)).await;
                    }
                    Err((error, _)) => break Err(error),
                }
            };

            (seqno, result, metadata)
        }));

        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.poll_delivery_fut(cx));

        let this = Pin::into_inner(self);
        while !this.in_flight.is_empty() {
            match ready!(Pin::new(&mut this.in_flight).poll_next(cx)) {
                Some((seqno, Ok(result), metadata)) => {
                    match result {
                        Ok((partition, offset)) => {
                            metadata.update_status(EventStatus::Delivered);
                            trace!(message = "Produced message.", ?partition, ?offset);
                        }
                        Err(error) => {
                            metadata.update_status(EventStatus::Errored);
                            error!(message = "Kafka error.", %error);
                        }
                    }

                    this.pending_acks.insert(seqno);

                    let mut num_to_ack = 0;
                    while this.pending_acks.remove(&this.seq_tail) {
                        num_to_ack += 1;
                        this.seq_tail += 1
                    }
                    this.acker.ack(num_to_ack);
                }
                Some((_, Err(Canceled), metadata)) => {
                    error!(message = "Request canceled.");
                    metadata.update_status(EventStatus::Errored);
                    return Poll::Ready(Err(()));
                }
                None => break,
            }
        }

        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}

fn get_headers(event: &Event, headers_key: &str) -> Option<OwnedHeaders> {
    if let Event::Log(log) = event {
        if let Some(headers) = log.get(headers_key) {
            match headers {
                Value::Map(headers_map) => {
                    let mut owned_headers = OwnedHeaders::new_with_capacity(headers_map.len());
                    for (key, value) in headers_map {
                        if let Value::Bytes(value_bytes) = value {
                            owned_headers = owned_headers.add(key, value_bytes.as_ref());
                        } else {
                            emit!(&KafkaHeaderExtractionFailed {
                                header_field: headers_key
                            });
                        }
                    }
                    return Some(owned_headers);
                }
                _ => {
                    emit!(&KafkaHeaderExtractionFailed {
                        header_field: headers_key
                    });
                }
            }
        }
    }
    None
}

async fn healthcheck(config: KafkaSinkConfig) -> crate::Result<()> {
    trace!("Healthcheck started.");
    let client = config.to_rdkafka(KafkaRole::Consumer).unwrap();
    let topic = match Template::try_from(config.topic)
        .context(TopicTemplate)?
        .render_string(&Event::from(""))
    {
        Ok(topic) => Some(topic),
        Err(error) => {
            warn!(
                message = "Could not generate topic for healthcheck.",
                %error,
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
    trace!("Healthcheck completed.");
    Ok(())
}

fn encode_event(
    mut event: Event,
    key_field: &Option<String>,
    encoding: &EncodingConfig<Encoding>,
) -> (Vec<u8>, Vec<u8>, EventMetadata) {
    let key = key_field
        .as_ref()
        .and_then(|f| match &event {
            Event::Log(log) => log.get(f).map(|value| value.as_bytes().to_vec()),
            Event::Metric(metric) => metric
                .tags()
                .and_then(|tags| tags.get(f))
                .map(|value| value.clone().into_bytes()),
        })
        .unwrap_or_default();

    encoding.apply_rules(&mut event);

    let body = match &event {
        Event::Log(log) => match encoding.codec() {
            Encoding::Json => serde_json::to_vec(&log).unwrap(),
            Encoding::Text => log
                .get(log_schema().message_key())
                .map(|v| v.as_bytes().to_vec())
                .unwrap_or_default(),
        },
        Event::Metric(metric) => match encoding.codec() {
            Encoding::Json => serde_json::to_vec(&metric).unwrap(),
            Encoding::Text => metric.to_string().into_bytes(),
        },
    };

    let metadata = event.into_metadata();
    (key, body, metadata)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use rdkafka::message::Headers;

    use super::*;
    use crate::event::{Metric, MetricKind, MetricValue};
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KafkaSinkConfig>();
    }

    #[test]
    fn kafka_encode_event_log_text() {
        crate::test_util::trace_init();
        let key = "";
        let message = "hello world".to_string();
        let (key_bytes, bytes, _metadata) = encode_event(
            message.clone().into(),
            &None,
            &EncodingConfig::from(Encoding::Text),
        );

        assert_eq!(&key_bytes[..], key.as_bytes());
        assert_eq!(&bytes[..], message.as_bytes());
    }

    #[test]
    fn kafka_encode_event_log_json() {
        crate::test_util::trace_init();
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        event.as_mut_log().insert("foo", "bar");

        let (key, bytes, _metadata) = encode_event(
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
    fn kafka_encode_event_metric_text() {
        let metric = Metric::new(
            "kafka-metric",
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.0 },
        );
        let (key_bytes, bytes, _metadata) = encode_event(
            metric.clone().into(),
            &None,
            &EncodingConfig::from(Encoding::Text),
        );

        assert_eq!("", String::from_utf8_lossy(&key_bytes));
        assert_eq!(metric.to_string(), String::from_utf8_lossy(&bytes));
    }

    #[test]
    fn kafka_encode_event_metric_json() {
        let metric = Metric::new(
            "kafka-metric",
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.0 },
        );
        let (key_bytes, bytes, _metadata) = encode_event(
            metric.clone().into(),
            &None,
            &EncodingConfig::from(Encoding::Json),
        );

        assert_eq!("", String::from_utf8_lossy(&key_bytes));
        assert_eq!(
            serde_json::to_string(&metric).unwrap(),
            String::from_utf8_lossy(&bytes)
        );
    }

    #[test]
    fn kafka_encode_event_log_apply_rules() {
        crate::test_util::trace_init();
        let mut event = Event::from("hello");
        event.as_mut_log().insert("key", "value");

        let (key, bytes, _metadata) = encode_event(
            event,
            &Some("key".into()),
            &EncodingConfig {
                codec: Encoding::Json,
                schema: None,
                only_fields: None,
                except_fields: Some(vec!["key".into()]),
                timestamp_format: None,
            },
        );

        let map: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();

        assert_eq!(&key[..], b"value");
        assert!(!map.contains_key("key"));
    }

    #[test]
    fn kafka_get_headers() {
        let headers_key = "headers";
        let mut header_values = BTreeMap::new();
        header_values.insert("a-key".to_string(), Value::Bytes(Bytes::from("a-value")));
        header_values.insert("b-key".to_string(), Value::Bytes(Bytes::from("b-value")));

        let mut event = Event::from("hello");
        event.as_mut_log().insert(headers_key, header_values);

        let headers = get_headers(&event, headers_key).unwrap();
        assert_eq!(headers.get(0).unwrap().0, "a-key");
        assert_eq!(headers.get(0).unwrap().1, "a-value".as_bytes());
        assert_eq!(headers.get(1).unwrap().0, "b-key");
        assert_eq!(headers.get(1).unwrap().1, "b-value".as_bytes());
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
    use bytes::Bytes;
    use futures::StreamExt;
    use rdkafka::{
        consumer::{BaseConsumer, Consumer},
        message::Headers,
        Message, Offset, TopicPartitionList,
    };
    use std::{collections::BTreeMap, future::ready, thread, time::Duration};
    use vector_core::event::{BatchNotifier, BatchStatus};

    #[tokio::test]
    async fn healthcheck() {
        crate::test_util::trace_init();
        let topic = format!("test-{}", random_string(10));

        let config = KafkaSinkConfig {
            bootstrap_servers: "localhost:9091".into(),
            topic: topic.clone(),
            key_field: None,
            encoding: EncodingConfig::from(Encoding::Text),
            batch: BatchConfig::default(),
            compression: KafkaCompression::None,
            auth: KafkaAuthConfig::default(),
            socket_timeout_ms: 60000,
            message_timeout_ms: 300000,
            librdkafka_options: HashMap::new(),
            headers_key: None,
        };

        super::healthcheck(config).await.unwrap();
    }

    #[tokio::test]
    async fn kafka_happy_path_plaintext() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::None).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_gzip() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Gzip).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_lz4() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Lz4).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_snappy() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Snappy).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_zstd() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Zstd).await;
    }

    async fn kafka_batch_options_overrides(
        batch: BatchConfig,
        librdkafka_options: HashMap<String, String>,
    ) -> crate::Result<KafkaSink> {
        let topic = format!("test-{}", random_string(10));
        let config = KafkaSinkConfig {
            bootstrap_servers: "localhost:9091".to_string(),
            topic: format!("{}-%Y%m%d", topic),
            compression: KafkaCompression::None,
            encoding: Encoding::Text.into(),
            key_field: None,
            auth: KafkaAuthConfig {
                sasl: None,
                tls: None,
            },
            socket_timeout_ms: 60000,
            message_timeout_ms: 300000,
            batch,
            librdkafka_options,
            headers_key: None,
        };
        let (acker, _ack_counter) = Acker::new_for_testing();
        config.clone().to_rdkafka(KafkaRole::Consumer)?;
        config.clone().to_rdkafka(KafkaRole::Producer)?;
        super::healthcheck(config.clone()).await?;
        KafkaSink::new(config, acker)
    }

    #[tokio::test]
    async fn kafka_batch_options_max_bytes_errors_on_double_set() {
        crate::test_util::trace_init();
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
        .await
        .is_err())
    }

    #[tokio::test]
    async fn kafka_batch_options_actually_sets() {
        crate::test_util::trace_init();
        kafka_batch_options_overrides(
            BatchConfig {
                max_bytes: None,
                max_events: Some(10),
                max_size: None,
                timeout_secs: Some(2),
            },
            indexmap::indexmap! {}.into_iter().collect(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn kafka_batch_options_max_events_errors_on_double_set() {
        crate::test_util::trace_init();
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
        .await
        .is_err())
    }

    #[tokio::test]
    async fn kafka_batch_options_timeout_secs_errors_on_double_set() {
        crate::test_util::trace_init();
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
        .await
        .is_err())
    }

    #[tokio::test]
    async fn kafka_happy_path_tls() {
        crate::test_util::trace_init();
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
        crate::test_util::trace_init();
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
        crate::test_util::trace_init();
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
        let headers_key = "headers_key".to_string();
        let kafka_auth = KafkaAuthConfig { sasl, tls };
        let config = KafkaSinkConfig {
            bootstrap_servers: server.to_string(),
            topic: format!("{}-%Y%m%d", topic),
            key_field: None,
            encoding: EncodingConfig::from(Encoding::Text),
            batch: BatchConfig::default(),
            compression,
            auth: kafka_auth.clone(),
            socket_timeout_ms: 60000,
            message_timeout_ms: 300000,
            librdkafka_options: HashMap::new(),
            headers_key: Some(headers_key.clone()),
        };
        let topic = format!("{}-{}", topic, chrono::Utc::now().format("%Y%m%d"));
        let (acker, ack_counter) = Acker::new_for_testing();
        let sink = KafkaSink::new(config, acker).unwrap();

        let num_events = 1000;
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input, events) = random_lines_with_stream(100, num_events, Some(batch));

        let header_1_key = "header-1-key";
        let header_1_value = "header-1-value";
        events
            .map(|mut event| {
                let mut header_values = BTreeMap::new();
                header_values.insert(
                    header_1_key.to_string(),
                    Value::Bytes(Bytes::from(header_1_value)),
                );
                event
                    .as_mut_log()
                    .insert(headers_key.clone(), header_values);
                Ok(event)
            })
            .forward(sink)
            .await
            .unwrap();
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        // read back everything from the beginning
        let mut client_config = rdkafka::ClientConfig::new();
        client_config.set("bootstrap.servers", server);
        client_config.set("group.id", &random_string(10));
        client_config.set("enable.partition.eof", "true");
        let _ = kafka_auth.apply(&mut client_config).unwrap();

        let mut tpl = TopicPartitionList::new();
        tpl.add_partition(&topic, 0)
            .set_offset(Offset::Beginning)
            .unwrap();

        let consumer: BaseConsumer = client_config.create().unwrap();
        consumer.assign(&tpl).unwrap();

        // wait for messages to show up
        wait_for(
            || match consumer.fetch_watermarks(&topic, 0, Duration::from_secs(3)) {
                Ok((_low, high)) => ready(high > 0),
                Err(err) => {
                    println!("retrying due to error fetching watermarks: {}", err);
                    ready(false)
                }
            },
        )
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
                    let (header_key, header_val) = msg.headers().unwrap().get(0).unwrap();
                    assert_eq!(header_key, header_1_key);
                    assert_eq!(header_val, header_1_value.as_bytes());
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
