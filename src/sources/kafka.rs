use crate::{
    event::{self, Event},
    kafka::KafkaAuthConfig,
    shutdown::ShutdownSignal,
    stream::StreamExt,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes::Bytes;
use futures::compat::Compat;
use futures01::{future, sync::mpsc, Future, Poll, Sink, Stream};
use owning_ref::OwningHandle;
use rdkafka::{
    config::ClientConfig,
    consumer::{Consumer, DefaultConsumerContext, MessageStream, StreamConsumer},
    error::KafkaError,
    message::{BorrowedMessage, Message},
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{collections::HashMap, sync::Arc};
use tokio::task::block_in_place;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not create Kafka consumer: {}", source))]
    KafkaCreateError { source: rdkafka::error::KafkaError },
    #[snafu(display("Could not subscribe to Kafka topics: {}", source))]
    KafkaSubscribeError { source: rdkafka::error::KafkaError },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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
    key_field: Option<String>,
    librdkafka_options: Option<HashMap<String, String>>,
    #[serde(flatten)]
    auth: KafkaAuthConfig,
}

fn default_session_timeout_ms() -> u64 {
    10000 // default in librdkafka
}

fn default_socket_timeout_ms() -> u64 {
    60000 // default in librdkafka
}

fn default_fetch_wait_max_ms() -> u64 {
    100 // default in librdkafka
}

fn default_commit_interval_ms() -> u64 {
    5000 // default in librdkafka
}

fn default_auto_offset_reset() -> String {
    "largest".into() // default in librdkafka
}

inventory::submit! {
    SourceDescription::new_without_default::<KafkaSourceConfig>("kafka")
}

#[typetag::serde(name = "kafka")]
impl SourceConfig for KafkaSourceConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        kafka_source(self.clone(), shutdown, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "kafka"
    }
}

fn kafka_source(
    config: KafkaSourceConfig,
    shutdown: ShutdownSignal,
    out: mpsc::Sender<Event>,
) -> crate::Result<super::Source> {
    let consumer = Arc::new(create_consumer(config.clone())?);
    let source = future::lazy(move || {
        let consumer_ref = Arc::clone(&consumer);

        // See https://github.com/fede1024/rust-rdkafka/issues/85#issuecomment-439141656
        let stream = OwnedConsumerStream {
            upstream: OwningHandle::new_with_fn(consumer.clone(), |c| {
                let cf = unsafe { &*c };
                Box::new(Compat::new(cf.start()))
            }),
        };

        stream
            .take_until(shutdown.map(move |_| block_in_place(|| consumer.stop())))
            .then(move |message| {
                match message {
                    Err(e) => Err(error!(message = "Error reading message from Kafka", error = ?e)),
                    Ok(msg) => {
                        let payload = match msg.payload_view::<[u8]>() {
                            None => return Err(()), // skip messages with empty payload
                            Some(Err(e)) => {
                                return Err(error!(message = "Cannot extract payload", error = ?e))
                            }
                            Some(Ok(payload)) => Bytes::from(payload),
                        };
                        let mut event = Event::from(payload);

                        // Add source type
                        event
                            .as_mut_log()
                            .insert(event::log_schema().source_type_key(), "kafka");

                        if let Some(key_field) = &config.key_field {
                            match msg.key_view::<[u8]>() {
                                None => (),
                                Some(Err(e)) => {
                                    return Err(error!(message = "Cannot extract key", error = ?e))
                                }
                                Some(Ok(key)) => {
                                    event.as_mut_log().insert(key_field.clone(), key);
                                }
                            }
                        }

                        consumer_ref.store_offset(&msg).map_err(
                            |e| error!(message = "Cannot store offset for the message", error = ?e),
                        )?;
                        Ok(event)
                    }
                }
            })
            .forward(out.sink_map_err(|e| error!(message = "Error sending to sink", error = ?e)))
            .map(|_| ())
    });

    Ok(Box::new(source))
}

fn create_consumer(config: KafkaSourceConfig) -> crate::Result<StreamConsumer> {
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
        .set("client.id", "vector");

    config.auth.apply(&mut client_config)?;

    if let Some(librdkafka_options) = config.librdkafka_options {
        for (key, value) in librdkafka_options.into_iter() {
            client_config.set(key.as_str(), value.as_str());
        }
    }

    let consumer: StreamConsumer = client_config.create().context(KafkaCreateError)?;
    let topics: Vec<&str> = config.topics.iter().map(|s| s.as_str()).collect();
    consumer.subscribe(&topics).context(KafkaSubscribeError)?;

    Ok(consumer)
}

struct OwnedConsumerStream {
    upstream: OwningHandle<
        Arc<StreamConsumer>,
        Box<Compat<MessageStream<'static, DefaultConsumerContext>>>,
    >,
}

impl Stream for OwnedConsumerStream {
    type Item = BorrowedMessage<'static>;
    type Error = KafkaError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.upstream.poll()
    }
}

#[cfg(test)]
mod test {
    use super::{kafka_source, KafkaSourceConfig};
    use crate::shutdown::ShutdownSignal;
    use futures01::sync::mpsc;

    fn make_config() -> KafkaSourceConfig {
        KafkaSourceConfig {
            bootstrap_servers: "localhost:9092".to_string(),
            topics: vec!["my-topic".to_string()],
            group_id: "group-id".to_string(),
            auto_offset_reset: "earliest".to_string(),
            session_timeout_ms: 10000,
            commit_interval_ms: 5000,
            key_field: Some("message_key".to_string()),
            socket_timeout_ms: 60000,
            fetch_wait_max_ms: 100,
            ..Default::default()
        }
    }

    #[test]
    fn kafka_source_create_ok() {
        let config = make_config();
        assert!(kafka_source(config, ShutdownSignal::noop(), mpsc::channel(1).0).is_ok());
    }

    #[test]
    fn kafka_source_create_incorrect_auto_offset_reset() {
        let config = KafkaSourceConfig {
            auto_offset_reset: "incorrect-auto-offset-reset".to_string(),
            ..make_config()
        };
        assert!(kafka_source(config, ShutdownSignal::noop(), mpsc::channel(1).0).is_err());
    }
}

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use super::{kafka_source, KafkaSourceConfig};
    use crate::{
        event,
        shutdown::ShutdownSignal,
        test_util::{collect_n, random_string, runtime},
    };
    use futures::compat::Compat;
    use futures01::{sync::mpsc, Future};
    use rdkafka::{
        config::ClientConfig,
        producer::{FutureProducer, FutureRecord},
    };
    use string_cache::DefaultAtom as Atom;

    const BOOTSTRAP_SERVER: &str = "localhost:9092";

    fn send_event(topic: &str, key: &str, text: &str) -> impl Future<Item = (), Error = ()> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", BOOTSTRAP_SERVER)
            .set("produce.offset.report", "true")
            .set("message.timeout.ms", "5000")
            .create()
            .expect("Producer creation error");

        let record = FutureRecord::to(topic).payload(text).key(key);

        Compat::new(producer.send(record, 0))
            .map(|_| ())
            .map_err(|e| panic!("Cannot send event to Kafka: {:?}", e))
    }

    #[test]
    #[ignore]
    fn kafka_source_consume_event() {
        let topic = format!("test-topic-{}", random_string(10));
        println!("Test topic name: {}", topic);
        let group_id = format!("test-group-{}", random_string(10));

        let config = KafkaSourceConfig {
            bootstrap_servers: BOOTSTRAP_SERVER.into(),
            topics: vec![topic.clone()],
            group_id: group_id.clone(),
            auto_offset_reset: "beginning".into(),
            session_timeout_ms: 6000,
            commit_interval_ms: 5000,
            key_field: Some("message_key".to_string()),
            socket_timeout_ms: 60000,
            fetch_wait_max_ms: 100,
            ..Default::default()
        };

        let mut rt = runtime();
        println!("Sending event...");
        rt.block_on(send_event(&topic, "my key", "my message"))
            .unwrap();
        println!("Receiving event...");
        let (tx, rx) = mpsc::channel(1);
        rt.spawn(kafka_source(config, ShutdownSignal::noop(), tx).unwrap());
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();
        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "my message".into()
        );
        assert_eq!(
            events[0].as_log()[&Atom::from("message_key")],
            "my key".into()
        );
        assert_eq!(
            events[0].as_log()[event::log_schema().source_type_key()],
            "kafka".into()
        );
    }
}
