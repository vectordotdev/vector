use crate::{
    buffers::Acker,
    event::{self, Event},
    sinks::util::MetadataFuture,
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
use std::collections::HashSet;
use std::time::Duration;
use string_cache::DefaultAtom as Atom;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KafkaSinkConfig {
    bootstrap_servers: Vec<String>,
    topic: String,
    key_field: Option<Atom>,
    encoding: Option<Encoding>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

pub struct KafkaSink {
    producer: FutureProducer,
    topic: String,
    key_field: Option<Atom>,
    encoding: Option<Encoding>,
    in_flight: FuturesUnordered<MetadataFuture<DeliveryFuture, usize>>,

    acker: Acker,
    seq_head: usize,
    seq_tail: usize,
    pending_acks: HashSet<usize>,
}

#[typetag::serde(name = "kafka")]
impl crate::topology::config::SinkConfig for KafkaSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = KafkaSink::new(self.clone(), acker)?;
        let hc = healthcheck(self.clone());
        Ok((Box::new(sink), hc))
    }
}

impl KafkaSinkConfig {
    fn to_rdkafka(&self) -> rdkafka::ClientConfig {
        let mut client_config = rdkafka::ClientConfig::new();
        let bs = self.bootstrap_servers.join(",");
        client_config.set("bootstrap.servers", &bs);
        client_config
    }
}

impl KafkaSink {
    fn new(config: KafkaSinkConfig, acker: Acker) -> Result<Self, String> {
        config
            .to_rdkafka()
            .create()
            .map_err(|e| format!("error creating kafka producer: {}", e))
            .map(|producer| KafkaSink {
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
    let consumer: BaseConsumer = config.to_rdkafka().create().unwrap();

    let check = poll_fn(move || {
        tokio_threadpool::blocking(|| {
            consumer
                .fetch_metadata(Some(&config.topic), Duration::from_secs(3))
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    })
    .map_err(|e| e.to_string())
    .and_then(|result| result.into_future());

    Box::new(check)
}

fn encode_event(
    event: &Event,
    key_field: &Option<Atom>,
    encoding: &Option<Encoding>,
) -> (Vec<u8>, Vec<u8>) {
    let log = event.as_log();

    let body = match (encoding, log.is_structured()) {
        (&Some(Encoding::Json), _) | (_, true) => serde_json::to_vec(&log.all_fields()).unwrap(),
        (&Some(Encoding::Text), _) | (_, false) => log
            .get(&event::MESSAGE)
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or(Vec::new()),
    };

    let key = key_field
        .as_ref()
        .and_then(|f| log.get(f))
        .map(|v| v.as_bytes().to_vec())
        .unwrap_or(Vec::new());

    (key, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};
    use std::collections::HashMap;

    #[test]
    fn kafka_encode_event_non_structured() {
        let key = "";
        let message = "hello world".to_string();
        let (key_bytes, bytes) = encode_event(&message.clone().into(), &None, &None);

        assert_eq!(&key_bytes[..], key.as_bytes());
        assert_eq!(&bytes[..], message.as_bytes());
    }

    #[test]
    fn kafka_encode_event_structured() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event
            .as_mut_log()
            .insert_explicit("key".into(), "value".into());
        event
            .as_mut_log()
            .insert_explicit("foo".into(), "bar".into());

        let (key, bytes) = encode_event(&event, &Some("key".into()), &None);

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
    use super::{KafkaSink, KafkaSinkConfig};
    use crate::buffers::Acker;
    use crate::test_util::{block_on, random_lines_with_stream, random_string, wait_for};
    use futures::Sink;
    use rdkafka::{
        consumer::{BaseConsumer, Consumer},
        Message, Offset, TopicPartitionList,
    };
    use std::{thread, time::Duration};

    #[test]
    fn kafka_happy_path() {
        let bootstrap_servers = vec![String::from("localhost:9092")];
        let topic = format!("test-{}", random_string(10));

        let config = KafkaSinkConfig {
            bootstrap_servers: bootstrap_servers.clone(),
            topic: topic.clone(),
            key_field: None,
            encoding: None,
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
