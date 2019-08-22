use crate::{
    event::{self, Event},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bytes::Bytes;
use futures::{future, sync::mpsc, Future, Poll, Sink, Stream};
use owning_ref::OwningHandle;
use rdkafka::{
    config::ClientConfig,
    consumer::{Consumer, DefaultConsumerContext, MessageStream, StreamConsumer},
    error::KafkaResult,
    message::{BorrowedMessage, Message},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KafkaSourceConfig {
    bootstrap_servers: String,
    topics: Vec<String>,
    group_id: String,
    #[serde(default = "default_auto_offset_reset")]
    auto_offset_reset: String,
    #[serde(default = "default_session_timeout_ms")]
    session_timeout_ms: u64,
    host_key: Option<String>,
    key_field: Option<String>,
}

fn default_session_timeout_ms() -> u64 {
    10000 // default in librdkafka
}

fn default_auto_offset_reset() -> String {
    "largest".into() // default in librdkafka
}

#[typetag::serde(name = "kafka")]
impl SourceConfig for KafkaSourceConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> Result<super::Source, String> {
        kafka_source(self.clone(), out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

fn kafka_source(
    config: KafkaSourceConfig,
    out: mpsc::Sender<Event>,
) -> Result<super::Source, String> {
    let consumer = Arc::new(create_consumer(config.clone())?);
    let source = future::lazy(move || {
        let hostname = hostname::get_hostname();
        let host_key = config.host_key.clone().unwrap_or(event::HOST.to_string());
        let consumer_ref = Arc::clone(&consumer);

        // See https://github.com/fede1024/rust-rdkafka/issues/85#issuecomment-439141656
        let stream = OwnedConsumerStream {
            upstream: OwningHandle::new_with_fn(consumer, |c| {
                let cf = unsafe { &*c };
                Box::new(cf.start())
            }),
        };

        stream
            .then(move |message| {
                match message {
                    Err(e) => Err(error!(message = "Error reading message from Kafka", error = ?e)),
                    Ok(Err(e)) => Err(error!(message = "Kafka returned error", error = ?e)),
                    Ok(Ok(msg)) => {
                        let payload = match msg.payload_view::<[u8]>() {
                            None => return Err(()), // skip messages with empty payload
                            Some(Err(e)) => {
                                return Err(error!(message = "Cannot extract payload", error = ?e))
                            }
                            Some(Ok(payload)) => Bytes::from(payload),
                        };

                        let message_key = if config.key_field.is_some() {
                            match msg.key_view::<[u8]>() {
                                None => None,
                                Some(Err(e)) => {
                                    return Err(error!(message = "Cannot extract key", error = ?e))
                                }
                                Some(Ok(key)) => Some(Bytes::from(key)),
                            }
                        } else {
                            None
                        };

                        let event = create_event(
                            payload,
                            &config.key_field,
                            message_key,
                            &host_key,
                            &hostname,
                        );
                        consumer_ref
                            .store_offset(&msg)
                            .map_err(|e| error!(message = "Cannot store offset", error = ?e))?;
                        Ok(event)
                    }
                }
            })
            .forward(out.sink_map_err(|e| error!(message = "Error sending to sink", error = ?e)))
            .map(|_| ())
    });

    Ok(Box::new(source))
}

fn create_event(
    payload: Bytes,
    key_field: &Option<String>,
    message_key: Option<Bytes>,
    host_key: &str,
    hostname: &Option<String>,
) -> event::Event {
    let mut event = Event::from(payload);

    if let Some(key_field) = key_field {
        if let Some(message_key) = message_key {
            event
                .as_mut_log()
                .insert_implicit(key_field.clone().into(), message_key.into());
        }
    }

    if let Some(hostname) = &hostname {
        event
            .as_mut_log()
            .insert_implicit(host_key.clone().into(), hostname.clone().into());
    }

    event
}

fn create_consumer(config: KafkaSourceConfig) -> Result<StreamConsumer, String> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("group.id", &config.group_id)
        .set("bootstrap.servers", &config.bootstrap_servers)
        .set("auto.offset.reset", &config.auto_offset_reset)
        .set("session.timeout.ms", &config.session_timeout_ms.to_string())
        .set("enable.partition.eof", "false")
        .set("enable.auto.commit", "false")
        .set("client.id", "vector")
        .create()
        .map_err(|e| format!("Cannot create Kafka consumer: {:?}", e))?;

    let topics: Vec<&str> = config.topics.iter().map(|s| s.as_str()).collect();
    consumer
        .subscribe(&topics)
        .map_err(|e| format!("Cannot subscribe to topics: {:?}", e))?;

    Ok(consumer)
}

struct OwnedConsumerStream {
    upstream:
        OwningHandle<Arc<StreamConsumer>, Box<MessageStream<'static, DefaultConsumerContext>>>,
}

impl Stream for OwnedConsumerStream {
    type Item = KafkaResult<BorrowedMessage<'static>>;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.upstream.poll()
    }
}

#[cfg(test)]
mod test {
    use super::{create_event, kafka_source, KafkaSourceConfig};
    use crate::event;
    use futures::sync::mpsc;
    use string_cache::DefaultAtom as Atom;

    fn make_config() -> KafkaSourceConfig {
        KafkaSourceConfig {
            bootstrap_servers: "localhost:9092".to_string(),
            topics: vec!["my-topic".to_string()],
            group_id: "group-id".to_string(),
            auto_offset_reset: "earliest".to_string(),
            session_timeout_ms: 10000,
            host_key: None,
            key_field: Some("message_key".to_string()),
        }
    }

    #[test]
    fn kafka_source_create_ok() {
        let config = make_config();
        assert!(kafka_source(config, mpsc::channel(1).0).is_ok());
    }

    #[test]
    fn kafka_source_create_incorrect_auto_offset_reset() {
        let config = KafkaSourceConfig {
            auto_offset_reset: "incorrect-auto-offset-reset".to_string(),
            ..make_config()
        };
        assert!(kafka_source(config, mpsc::channel(1).0).is_err());
    }

    #[test]
    fn kafka_source_create_event() {
        let event = create_event(
            "my message".into(),
            &Some("message_key".to_string()),
            Some("my key".into()),
            &event::HOST.to_string(),
            &Some("my hostname".to_string()),
        );
        assert_eq!(event.as_log()[&event::MESSAGE], "my message".into());
        assert_eq!(event.as_log()[&event::HOST], "my hostname".into());
        assert_eq!(event.as_log()[&Atom::from("message_key")], "my key".into());
    }

    #[test]
    fn kafka_source_create_event_no_hostname() {
        let event = create_event(
            "my message".into(),
            &Some("message_key".to_string()),
            Some("my key".into()),
            &event::HOST.to_string(),
            &None,
        );
        assert_eq!(event.as_log()[&event::MESSAGE], "my message".into());
        assert_eq!(event.as_log().get(&event::HOST), None);
        assert_eq!(event.as_log()[&Atom::from("message_key")], "my key".into());
    }

    #[test]
    fn kafka_source_create_event_no_key_field() {
        let event = create_event(
            "my message".into(),
            &None,
            None,
            &event::HOST.to_string(),
            &Some("my hostname".to_string()),
        );
        assert_eq!(event.as_log()[&event::MESSAGE], "my message".into());
        assert_eq!(event.as_log()[&event::HOST], "my hostname".into());
        assert_eq!(event.as_log().get(&Atom::from("message_key")), None);
    }

    #[test]
    fn kafka_source_create_event_empty_key() {
        let event = create_event(
            "my message".into(),
            &Some("message_key".to_string()),
            None,
            &event::HOST.to_string(),
            &Some("my hostname".to_string()),
        );
        assert_eq!(event.as_log()[&event::MESSAGE], "my message".into());
        assert_eq!(event.as_log()[&event::HOST], "my hostname".into());
        assert_eq!(event.as_log().get(&Atom::from("message_key")), None);
    }
}

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use super::{kafka_source, KafkaSourceConfig};
    use crate::{
        event,
        test_util::{collect_n, random_string, runtime},
    };
    use futures::{sync::mpsc, Future};
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

        producer
            .send(record, 0)
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
            host_key: None,
            key_field: Some("message_key".to_string()),
        };

        let mut rt = runtime();
        println!("Sending event...");
        rt.block_on(send_event(&topic, "my key", "my message"))
            .unwrap();
        println!("Receiving event...");
        let (tx, rx) = mpsc::channel(1);
        rt.spawn(kafka_source(config, tx).unwrap());
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();
        assert_eq!(events[0].as_log()[&event::MESSAGE], "my message".into());
        assert_eq!(
            events[0].as_log()[&Atom::from("message_key")],
            "my key".into()
        );
    }
}
