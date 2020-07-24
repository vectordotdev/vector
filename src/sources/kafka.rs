use crate::{
    event::{self, Event},
    kafka::KafkaAuthConfig,
    shutdown::ShutdownSignal,
    internal_events::{KafkaEventFailed, KafkaEventReceived, KafkaOffsetUpdateFailed},
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{
    compat::{Compat, Future01CompatExt},
    FutureExt, StreamExt,
};
use futures01::{sync::mpsc, Sink};
use rdkafka::{
    config::ClientConfig,
    consumer::{Consumer, StreamConsumer},
    message::Message,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{collections::HashMap, sync::Arc};

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
        kafka_source(self, shutdown, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "kafka"
    }
}

fn kafka_source(
    config: &KafkaSourceConfig,
    shutdown: ShutdownSignal,
    out: mpsc::Sender<Event>,
) -> crate::Result<super::Source> {
    let key_field = config.key_field.clone();
    let consumer = Arc::new(create_consumer(config)?);

    let fut = async move {
        Arc::clone(&consumer)
            .start()
            .take_until(shutdown.clone().compat())
            .then(move |message| {
                let key_field = key_field.clone();
                let consumer = Arc::clone(&consumer);

                async move {
                    match message {
                        Err(error) => {
                            emit!(KafkaEventFailed{ error: error.clone() });
                            Err(error!(message = "Error reading message from Kafka", error = ?error))
                        }
                        Ok(msg) => {
                            emit!(KafkaEventReceived{ byte_size: msg.payload_len() });

                            let payload = match msg.payload_view::<[u8]>() {
                                None => return Err(()), // skip messages with empty payload
                                Some(Err(e)) => {
                                    return Err(
                                        error!(message = "Cannot extract payload", error = ?e),
                                    )
                                }
                                Some(Ok(payload)) => Bytes::from(payload),
                            };
                            let mut event = Event::new_empty_log();
                            let log = event.as_mut_log();

                            log.insert(event::log_schema().message_key().clone(), payload);

                            // Extract timestamp from kafka message
                            let timestamp = msg
                                .timestamp()
                                .to_millis()
                                .and_then(|millis| Utc.timestamp_millis_opt(millis).latest())
                                .unwrap_or_else(Utc::now);
                            log.insert(event::log_schema().timestamp_key().clone(), timestamp);

                            // Add source type
                            log.insert(event::log_schema().source_type_key(), "kafka");

                            if let Some(key_field) = &key_field {
                                match msg.key_view::<[u8]>() {
                                    None => (),
                                    Some(Err(e)) => {
                                        return Err(
                                            error!(message = "Cannot extract key", error = ?e),
                                        )
                                    }
                                    Some(Ok(key)) => {
                                        log.insert(key_field.clone(), key);
                                    }
                                }
                            }

                            consumer.store_offset(&msg).map_err(|error| {
                                emit!(KafkaOffsetUpdateFailed { error: error.clone() });
                                error!(message = "Cannot store offset for the message", error = ?error)
                            })?;
                            Ok(event)
                        }
                    }
                }
            })
            // Try `forward` after removing old futures.
            // Error: implementation of `futures_core::stream::Stream` is not general enough
            // .forward(
            //     out.sink_compat()
            //         .sink_map_err(|e| error!(message = "Error sending to sink", error = ?e)),
            // )
            .for_each(|item| {
                let out = out.clone();
                async move {
                    if let Ok(item) = item {
                        if let Err(e) = out.send(item).compat().await {
                            error!(message = "Error sending to sink", error = ?e);
                        }
                    }
                }
            })
            .await;
        Ok(())
    };

    Ok(Box::new(Compat::new(fut.boxed())))
}

fn create_consumer(config: &KafkaSourceConfig) -> crate::Result<StreamConsumer> {
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

    if let Some(librdkafka_options) = &config.librdkafka_options {
        for (key, value) in librdkafka_options {
            client_config.set(key.as_str(), value.as_str());
        }
    }

    let consumer: StreamConsumer = client_config.create().context(KafkaCreateError)?;
    let topics: Vec<&str> = config.topics.iter().map(|s| s.as_str()).collect();
    consumer.subscribe(&topics).context(KafkaSubscribeError)?;

    Ok(consumer)
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
        assert!(kafka_source(&config, ShutdownSignal::noop(), mpsc::channel(1).0).is_ok());
    }

    #[test]
    fn kafka_source_create_incorrect_auto_offset_reset() {
        let config = KafkaSourceConfig {
            auto_offset_reset: "incorrect-auto-offset-reset".to_string(),
            ..make_config()
        };
        assert!(kafka_source(&config, ShutdownSignal::noop(), mpsc::channel(1).0).is_err());
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
    use chrono::Utc;
    use futures01::sync::mpsc;
    use rdkafka::{
        config::ClientConfig,
        producer::{FutureProducer, FutureRecord},
        util::Timeout,
    };
    use string_cache::DefaultAtom as Atom;

    const BOOTSTRAP_SERVER: &str = "localhost:9092";

    async fn send_event(topic: String, key: &str, text: &str, timestamp: i64) {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", BOOTSTRAP_SERVER)
            .set("produce.offset.report", "true")
            .set("message.timeout.ms", "5000")
            .create()
            .expect("Producer creation error");

        let record = FutureRecord::to(&topic)
            .payload(text)
            .key(key)
            .timestamp(timestamp);

        if let Err(err) = producer.send(record, Timeout::Never).await {
            panic!("Cannot send event to Kafka: {:?}", err);
        }
    }

    #[test]
    #[ignore]
    fn kafka_source_consume_event() {
        let topic = format!("test-topic-{}", random_string(10));
        println!("Test topic name: {}", topic);
        let group_id = format!("test-group-{}", random_string(10));
        let now = Utc::now();

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
        rt.block_on_std(send_event(
            topic.clone(),
            "my key",
            "my message",
            now.timestamp_millis(),
        ));
        println!("Receiving event...");
        let (tx, rx) = mpsc::channel(1);
        rt.spawn(kafka_source(&config, ShutdownSignal::noop(), tx).unwrap());
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
        assert_eq!(
            events[0].as_log()[event::log_schema().timestamp_key()],
            now.into()
        );
    }
}
