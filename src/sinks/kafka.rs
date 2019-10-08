use crate::{
    buffers::Acker,
    event::{self, Event},
    sinks::util::tls::TlsOptions,
    sinks::util::MetadataFuture,
    topology::config::{DataType, SinkConfig},
};
use futures::{
    future::{self, poll_fn, IntoFuture},
    stream::FuturesUnordered,
    Async, AsyncSink, Future, Poll, Sink, StartSend, Stream,
};
use rdkafka::{
    consumer::{BaseConsumer, Consumer},
    producer::{DeliveryFuture, FutureProducer, FutureRecord},
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use string_cache::DefaultAtom as Atom;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("creating kafka producer failed: {}", source))]
    KafkaCreateFailed { source: rdkafka::error::KafkaError },
    #[snafu(display("invalid path: {:?}", path))]
    InvalidPath { path: PathBuf },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KafkaSinkConfig {
    bootstrap_servers: Vec<String>,
    topic: String,
    key_field: Option<Atom>,
    encoding: Encoding,
    tls: Option<KafkaSinkTlsConfig>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KafkaSinkTlsConfig {
    enabled: Option<bool>,
    #[serde(flatten)]
    options: TlsOptions,
}

pub struct KafkaSink {
    producer: FutureProducer,
    topic: String,
    key_field: Option<Atom>,
    encoding: Encoding,
    in_flight: FuturesUnordered<MetadataFuture<DeliveryFuture, usize>>,

    acker: Acker,
    seq_head: usize,
    seq_tail: usize,
    pending_acks: HashSet<usize>,
}

#[typetag::serde(name = "kafka")]
impl SinkConfig for KafkaSinkConfig {
    fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = KafkaSink::new(self.clone(), acker)?;
        let hc = healthcheck(self.clone());
        Ok((Box::new(sink), hc))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }
}

impl KafkaSinkConfig {
    fn to_rdkafka(&self) -> crate::Result<rdkafka::ClientConfig> {
        let mut client_config = rdkafka::ClientConfig::new();
        let bs = self.bootstrap_servers.join(",");
        client_config.set("bootstrap.servers", &bs);
        if let Some(ref tls) = self.tls {
            let enabled = tls.enabled.unwrap_or(false);
            client_config.set(
                "security.protocol",
                if enabled { "ssl" } else { "plaintext" },
            );
            if let Some(ref path) = tls.options.ca_path {
                client_config.set("ssl.ca.location", pathbuf_to_string(&path)?);
            }
            if let Some(ref path) = tls.options.crt_path {
                client_config.set("ssl.certificate.location", pathbuf_to_string(&path)?);
            }
            if let Some(ref path) = tls.options.key_path {
                client_config.set("ssl.keystore.location", pathbuf_to_string(&path)?);
            }
            if let Some(ref pass) = tls.options.key_pass {
                client_config.set("ssl.keystore.password", pass);
            }
        }
        Ok(client_config)
    }
}

fn pathbuf_to_string(path: &PathBuf) -> crate::Result<&str> {
    path.to_str()
        .ok_or_else(|| BuildError::InvalidPath { path: path.into() }.into())
}

impl KafkaSink {
    fn new(config: KafkaSinkConfig, acker: Acker) -> crate::Result<Self> {
        let producer = config.to_rdkafka()?.create().context(KafkaCreateFailed)?;
        Ok(KafkaSink {
            producer,
            topic: config.topic,
            key_field: config.key_field,
            encoding: config.encoding,
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
        let topic = self.topic.clone();

        let (key, body) = encode_event(&item, &self.key_field, &self.encoding);

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

        self.in_flight.push(future.join(future::ok(seqno)));
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

fn healthcheck(config: KafkaSinkConfig) -> super::Healthcheck {
    let consumer: BaseConsumer = config.to_rdkafka().unwrap().create().unwrap();

    let check = poll_fn(move || {
        tokio_threadpool::blocking(|| {
            consumer
                .fetch_metadata(Some(&config.topic), Duration::from_secs(3))
                .map(|_| ())
                .map_err(|err| err.into())
        })
    })
    .map_err(|err| err.into())
    .and_then(|result| result.into_future());

    Box::new(check)
}

fn encode_event(
    event: &Event,
    key_field: &Option<Atom>,
    encoding: &Encoding,
) -> (Vec<u8>, Vec<u8>) {
    let key = key_field
        .as_ref()
        .and_then(|f| event.as_log().get(f))
        .map(|v| v.as_bytes().to_vec())
        .unwrap_or(Vec::new());

    let body = match encoding {
        &Encoding::Json => serde_json::to_vec(&event.as_log().clone().unflatten()).unwrap(),
        &Encoding::Text => event
            .as_log()
            .get(&event::MESSAGE)
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or(Vec::new()),
    };

    (key, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};
    use std::collections::HashMap;

    #[test]
    fn kafka_encode_event_text() {
        let key = "";
        let message = "hello world".to_string();
        let (key_bytes, bytes) = encode_event(&message.clone().into(), &None, &Encoding::Text);

        assert_eq!(&key_bytes[..], key.as_bytes());
        assert_eq!(&bytes[..], message.as_bytes());
    }

    #[test]
    fn kafka_encode_event_json() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event
            .as_mut_log()
            .insert_explicit("key".into(), "value".into());
        event
            .as_mut_log()
            .insert_explicit("foo".into(), "bar".into());

        let (key, bytes) = encode_event(&event, &Some("key".into()), &Encoding::Json);

        let map: HashMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();

        assert_eq!(&key[..], "value".as_bytes());
        assert_eq!(map[&event::MESSAGE.to_string()], message);
        assert_eq!(map["key"], "value".to_string());
        assert_eq!(map["foo"], "bar".to_string());
    }
}

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use super::*;
    use crate::buffers::Acker;
    use crate::sinks::util::tls::TlsOptions;
    use crate::test_util::{block_on, random_lines_with_stream, random_string, wait_for};
    use futures::Sink;
    use rdkafka::{
        consumer::{BaseConsumer, Consumer},
        Message, Offset, TopicPartitionList,
    };
    use std::{thread, time::Duration};

    #[test]
    fn kafka_happy_path_plaintext() {
        kafka_happy_path("localhost:9092", None);
    }

    const TEST_CA: &str = "tests/data/Vector_CA.crt";

    #[test]
    fn kafka_happy_path_tls() {
        kafka_happy_path(
            "localhost:9091",
            Some(KafkaSinkTlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    ca_path: Some(TEST_CA.into()),
                    ..Default::default()
                },
            }),
        );
    }

    fn kafka_happy_path(server: &str, tls: Option<KafkaSinkTlsConfig>) {
        let bootstrap_servers = vec![server.into()];
        let topic = format!("test-{}", random_string(10));

        let tls_enabled = tls
            .as_ref()
            .map(|tls| tls.enabled.unwrap_or(false))
            .unwrap_or(false);
        let config = KafkaSinkConfig {
            bootstrap_servers: bootstrap_servers.clone(),
            topic: topic.clone(),
            encoding: Encoding::Text,
            key_field: None,
            tls,
        };
        let (acker, ack_counter) = Acker::new_for_testing();
        let sink = KafkaSink::new(config, acker).unwrap();

        let num_events = 1000;
        let (input, events) = random_lines_with_stream(100, num_events);

        let pump = sink.send_all(events);
        block_on(pump).unwrap();

        // read back everything from the beginning
        let mut client_config = rdkafka::ClientConfig::new();
        let bs = bootstrap_servers.join(",");
        client_config.set("bootstrap.servers", &bs);
        client_config.set("group.id", &random_string(10));
        client_config.set("enable.partition.eof", "true");
        if tls_enabled {
            client_config.set("security.protocol", "ssl");
            client_config.set("ssl.ca.location", TEST_CA);
        }

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
