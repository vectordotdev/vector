use crate::{
    buffers::Acker,
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Value},
    kafka::{KafkaAuthConfig, KafkaCompression},
    serde::to_string,
    sinks::util::encoding::{EncodingConfig, EncodingConfigWithDefault, EncodingConfiguration},
    template::{Template, TemplateError},
};
use futures::{compat::Compat, FutureExt};
use futures01::{
    future as future01, stream::FuturesUnordered, Async, AsyncSink, Future, Poll, Sink, StartSend,
    Stream,
};
use rdkafka::{
    consumer::{BaseConsumer, Consumer},
    producer::{DeliveryFuture, FutureProducer, FutureRecord},
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::time::Duration;
use string_cache::DefaultAtom as Atom;

type MetadataFuture<F, M> = future01::Join<F, future01::FutureResult<M, <F as Future>::Error>>;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("creating kafka producer failed: {}", source))]
    KafkaCreateFailed { source: rdkafka::error::KafkaError },
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateError },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KafkaSinkConfig {
    bootstrap_servers: String,
    topic: String,
    key_field: Option<Atom>,
    encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    compression: KafkaCompression,
    #[serde(flatten)]
    auth: KafkaAuthConfig,
    #[serde(default = "default_socket_timeout_ms")]
    socket_timeout_ms: u64,
    #[serde(default = "default_message_timeout_ms")]
    message_timeout_ms: u64,
    librdkafka_options: Option<HashMap<String, String>>,
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
    producer: FutureProducer,
    topic: Template,
    key_field: Option<Atom>,
    encoding: EncodingConfig<Encoding>,
    in_flight: FuturesUnordered<MetadataFuture<Compat<DeliveryFuture>, usize>>,

    acker: Acker,
    seq_head: usize,
    seq_tail: usize,
    pending_acks: HashSet<usize>,
}

inventory::submit! {
    SinkDescription::new::<KafkaSinkConfig>("kafka")
}

impl GenerateConfig for KafkaSinkConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "kafka")]
impl SinkConfig for KafkaSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = KafkaSink::new(self.clone(), cx.acker())?;
        let hc = healthcheck(self.clone()).boxed();
        Ok((super::VectorSink::Futures01Sink(Box::new(sink)), hc))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "kafka"
    }
}

impl KafkaSinkConfig {
    fn to_rdkafka(&self) -> crate::Result<rdkafka::ClientConfig> {
        let mut client_config = rdkafka::ClientConfig::new();
        client_config
            .set("bootstrap.servers", &self.bootstrap_servers)
            .set("compression.codec", &to_string(self.compression))
            .set("socket.timeout.ms", &self.socket_timeout_ms.to_string())
            .set("message.timeout.ms", &self.message_timeout_ms.to_string());

        self.auth.apply(&mut client_config)?;

        if let Some(ref librdkafka_options) = self.librdkafka_options {
            for (key, value) in librdkafka_options.iter() {
                client_config.set(key.as_str(), value.as_str());
            }
        }

        Ok(client_config)
    }
}

impl KafkaSink {
    fn new(config: KafkaSinkConfig, acker: Acker) -> crate::Result<Self> {
        let producer = config.to_rdkafka()?.create().context(KafkaCreateFailed)?;
        Ok(KafkaSink {
            producer,
            topic: Template::try_from(config.topic).context(TopicTemplate)?,
            key_field: config.key_field,
            encoding: config.encoding.into(),
            in_flight: FuturesUnordered::new(),
            acker,
            seq_head: 0,
            seq_tail: 0,
            pending_acks: HashSet::new(),
        })
    }
}

impl Sink for KafkaSink {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let topic = self.topic.render_string(&item).map_err(|missing_keys| {
            error!(message = "Missing keys for topic", ?missing_keys);
        })?;

        let (key, body) = encode_event(item.clone(), &self.key_field, &self.encoding);

        let mut record = FutureRecord::to(&topic).key(&key).payload(&body[..]);

        if let Some(Value::Timestamp(timestamp)) =
            item.as_log().get(&Atom::from(log_schema().timestamp_key()))
        {
            record = record.timestamp(timestamp.timestamp_millis());
        }

        debug!(message = "sending event.", count = 1);
        let future = match self.producer.send_result(record) {
            Ok(f) => f,
            Err((e, record)) => {
                // Docs suggest this will only happen when the producer queue is full, so let's
                // treat it as we do full buffers in other sinks
                debug!("rdkafka queue full: {}", e);
                self.poll_complete()?;

                match self.producer.send_result(record) {
                    Ok(f) => f,
                    Err((e, _record)) => {
                        debug!("rdkafka queue still full: {}", e);
                        return Ok(AsyncSink::NotReady(item));
                    }
                }
            }
        };

        let seqno = self.seq_head;
        self.seq_head += 1;

        self.in_flight
            .push(Compat::new(future).join(future01::ok(seqno)));
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            match self.in_flight.poll() {
                // nothing ready yet
                Ok(Async::NotReady) => return Ok(Async::NotReady),

                // nothing in flight
                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),

                // request finished, check for success
                Ok(Async::Ready(Some((result, seqno)))) => {
                    match result {
                        Ok((partition, offset)) => trace!(
                            "Produced message to partition {} at offset {}",
                            partition,
                            offset
                        ),
                        Err((e, _msg)) => error!("Kafka error: {}", e),
                    };

                    self.pending_acks.insert(seqno);

                    let mut num_to_ack = 0;
                    while self.pending_acks.remove(&self.seq_tail) {
                        num_to_ack += 1;
                        self.seq_tail += 1
                    }
                    self.acker.ack(num_to_ack);
                }

                // request got canceled (according to docs)
                Err(e) => error!("delivery future canceled: {}", e),
            }
        }
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
                message = "Could not generate topic for healthcheck",
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
    key_field: &Option<Atom>,
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
            .get(&Atom::from(log_schema().message_key()))
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
                except_fields: Some(vec![Atom::from("key")]),
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
    use futures::{compat::Sink01CompatExt, future, SinkExt, StreamExt};
    use rdkafka::{
        consumer::{BaseConsumer, Consumer},
        Message, Offset, TopicPartitionList,
    };
    use std::{thread, time::Duration};

    const TEST_CA: &str = "tests/data/Vector_CA.crt";
    const TEST_CRT: &str = "tests/data/localhost.crt";
    const TEST_KEY: &str = "tests/data/localhost.key";

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

    #[tokio::test]
    async fn kafka_happy_path_tls() {
        kafka_happy_path(
            "localhost:9092",
            None,
            Some(KafkaTlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    ca_file: Some(TEST_CA.into()),
                    ..Default::default()
                },
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
                options: TlsOptions {
                    ca_file: Some(TEST_CA.into()),
                    // Dummy key, not actually checked by server
                    crt_file: Some(TEST_CRT.into()),
                    key_file: Some(TEST_KEY.into()),
                    ..Default::default()
                },
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
        let mut events = events.map(Ok);

        let _ = sink.sink_compat().send_all(&mut events).await.unwrap();

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
            future::ready(high > 0)
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
