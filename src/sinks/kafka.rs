use crate::{
    buffers::Acker,
    event::{self, Event},
    kafka::{KafkaAuthConfig, KafkaCompression},
    serde::to_string,
    sinks::util::encoding::{EncodingConfig, EncodingConfigWithDefault, EncodingConfiguration},
    template::{Template, TemplateError},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::compat::Compat;
use futures01::{
    future, stream::FuturesUnordered, Async, AsyncSink, Future, Poll, Sink, StartSend, Stream,
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

type MetadataFuture<F, M> = future::Join<F, future::FutureResult<M, <F as Future>::Error>>;

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
    SinkDescription::new_without_default::<KafkaSinkConfig>("kafka")
}

#[typetag::serde(name = "kafka")]
impl SinkConfig for KafkaSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = KafkaSink::new(self.clone(), cx.acker())?;
        let hc = healthcheck(self.clone())?;
        Ok((Box::new(sink), hc))
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
            ()
        })?;

        let (key, body) = encode_event(item.clone(), &self.key_field, &self.encoding);

        let record = FutureRecord::to(&topic).key(&key).payload(&body[..]);

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
            .push(Compat::new(future).join(future::ok(seqno)));
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
                            "produced message to partition {} at offset {}",
                            partition,
                            offset
                        ),
                        Err((e, _msg)) => error!("kafka error: {}", e),
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

fn healthcheck(config: KafkaSinkConfig) -> crate::Result<super::Healthcheck> {
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

    let check = future::lazy(move || {
        let consumer: BaseConsumer = client.create().unwrap();

        tokio::task::block_in_place(|| {
            let topic = topic.as_ref().map(|topic| &topic[..]);
            consumer
                .fetch_metadata(topic, Duration::from_secs(3))
                .map(|_| ())
                .map_err(|err| err.into())
        })
    });

    Ok(Box::new(check))
}

fn encode_event(
    mut event: Event,
    key_field: &Option<Atom>,
    encoding: &EncodingConfig<Encoding>,
) -> (Vec<u8>, Vec<u8>) {
    encoding.apply_rules(&mut event);
    let key = key_field
        .as_ref()
        .and_then(|f| event.as_log().get(f))
        .map(|v| v.as_bytes().to_vec())
        .unwrap_or_default();

    let body = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&event.as_log()).unwrap(),
        Encoding::Text => event
            .as_log()
            .get(&event::log_schema().message_key())
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    };

    (key, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};
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

        assert_eq!(&key[..], "value".as_bytes());
        assert_eq!(map[&event::log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
        assert_eq!(map["foo"], "bar".to_string());
    }
}

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use super::*;
    use crate::{
        buffers::Acker,
        kafka::{KafkaAuthConfig, KafkaSaslConfig, KafkaTlsConfig},
        test_util::{block_on, random_lines_with_stream, random_string, wait_for},
        tls::TlsOptions,
    };
    use futures::compat::Future01CompatExt;
    use futures01::Sink;
    use rdkafka::{
        consumer::{BaseConsumer, Consumer},
        Message, Offset, TopicPartitionList,
    };
    use std::{thread, time::Duration};

    const TEST_CA: &str = "tests/data/Vector_CA.crt";
    const TEST_CRT: &str = "tests/data/localhost.crt";
    const TEST_KEY: &str = "tests/data/localhost.key";

    #[test]
    fn healthcheck() {
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

        let mut rt = crate::test_util::runtime();
        let jh = rt.spawn_handle_std(super::healthcheck(config).unwrap().compat());

        rt.block_on_std(jh).unwrap().unwrap();
    }

    #[test]
    fn kafka_happy_path_plaintext() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::None);
    }

    #[test]
    fn kafka_happy_path_gzip() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Gzip);
    }

    #[test]
    fn kafka_happy_path_lz4() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Lz4);
    }

    #[test]
    fn kafka_happy_path_snappy() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Snappy);
    }

    #[test]
    fn kafka_happy_path_zstd() {
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Zstd);
    }

    #[test]
    fn kafka_happy_path_tls() {
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
        );
    }

    #[test]
    fn kafka_happy_path_tls_with_key() {
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
        );
    }

    #[test]
    fn kafka_happy_path_sasl() {
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
        );
    }

    fn kafka_happy_path(
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

        let pump = sink.send_all(events);
        let _ = block_on(pump).unwrap();

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
            high > 0
        });

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
