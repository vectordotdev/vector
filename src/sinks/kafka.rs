use crate::record::Record;
use futures::{
    future::{poll_fn, IntoFuture},
    stream::FuturesUnordered,
    Async, AsyncSink, Future, Poll, Sink, StartSend, Stream,
};
use log::{debug, error, trace};
use rdkafka::{
    consumer::{BaseConsumer, Consumer},
    producer::{DeliveryFuture, FutureProducer, FutureRecord},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct KafkaSinkConfig {
    bootstrap_servers: Vec<String>,
    topic: String,
}

struct KafkaSink {
    producer: FutureProducer,
    topic: String,
    in_flight: FuturesUnordered<DeliveryFuture>,
}

#[typetag::serde(name = "kafka")]
impl crate::topology::config::SinkConfig for KafkaSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = KafkaSink::new(self.clone())?;
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
    fn new(config: KafkaSinkConfig) -> Result<Self, String> {
        config
            .to_rdkafka()
            .create()
            .map_err(|e| format!("error creating kafka producer: {}", e))
            .map(|producer| KafkaSink {
                producer,
                topic: config.topic,
                in_flight: FuturesUnordered::new(),
            })
    }
}

impl Sink for KafkaSink {
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let topic = self.topic.clone();
        let record = FutureRecord::to(&topic).key(&()).payload(&item.line);

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

        self.in_flight.push(future);
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
                Ok(Async::Ready(Some(result))) => match result {
                    Ok((partition, offset)) => trace!(
                        "produced message to partition {} at offset {}",
                        partition,
                        offset
                    ),
                    Err((e, _msg)) => error!("kafka error: {}", e),
                },

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

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod test {
    use super::{KafkaSink, KafkaSinkConfig};
    use crate::{
        record::Record,
        test_util::{block_on, random_lines},
    };
    use futures::{future::poll_fn, stream, Sink};
    use rdkafka::{
        consumer::{BaseConsumer, Consumer},
        Message, Offset, TopicPartitionList,
    };
    use std::time::Duration;

    #[test]
    fn happy_path() {
        let bootstrap_servers = vec![String::from("localhost:9092")];
        let topic = format!("test-{}", random_lines(10).next().unwrap());

        let config = KafkaSinkConfig {
            bootstrap_servers: bootstrap_servers.clone(),
            topic: topic.clone(),
        };
        let sink = KafkaSink::new(config).unwrap();

        let num_records = 1000;
        let input = random_lines(100).take(num_records).collect::<Vec<_>>();

        let pump = sink.send_all(stream::iter_ok::<_, ()>(
            input.clone().into_iter().map(Record::from),
        ));

        let (mut sink, _) = block_on(pump).unwrap();
        block_on(poll_fn(move || sink.close())).unwrap();

        // read back everything from the beginning
        let mut client_config = rdkafka::ClientConfig::new();
        let bs = bootstrap_servers.join(",");
        client_config.set("bootstrap.servers", &bs);
        client_config.set("group.id", &random_lines(10).next().unwrap());
        client_config.set("enable.partition.eof", "true");

        let mut tpl = TopicPartitionList::new();
        tpl.add_partition(&topic, 0).set_offset(Offset::Beginning);

        let consumer: BaseConsumer = client_config.create().unwrap();
        consumer.assign(&tpl).unwrap();

        // check we have the expected number of messages in the topic
        let (low, high) = consumer
            .fetch_watermarks(&topic, 0, Duration::from_secs(3))
            .unwrap();
        assert_eq!((0, num_records as i64), (low, high));

        // loop instead of iter so we can set a timeout
        let mut out = Vec::new();
        while let Some(Ok(msg)) = consumer.poll(Duration::from_secs(3)) {
            let s: &str = msg.payload_view().unwrap().unwrap();
            out.push(s.to_owned());
        }

        assert_eq!(out.len(), input.len());
        assert_eq!(out, input);
    }
}
