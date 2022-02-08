use std::{
    collections::HashSet,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, ready, stream::FuturesUnordered, FutureExt, Sink, Stream};
use pulsar::{
    message::proto, producer::SendFuture, proto::CommandSendReceipt, Authentication,
    Error as PulsarError, Producer, Pulsar, TokioExecutor,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use vector_buffers::Acker;

use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    internal_events::PulsarEncodeEventFailed,
    sinks::util::encoding::{EncodingConfig, EncodingConfiguration},
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("creating pulsar producer failed: {}", source))]
    CreatePulsarSink { source: PulsarError },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PulsarSinkConfig {
    // Deprecated name
    #[serde(alias = "address")]
    endpoint: String,
    topic: String,
    encoding: EncodingConfig<Encoding>,
    auth: Option<AuthConfig>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthConfig {
    name: String,  // "token"
    token: String, // <jwt token>
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
    Avro,
}

type PulsarProducer = Producer<TokioExecutor>;
type BoxedPulsarProducer = Box<PulsarProducer>;

enum PulsarSinkState {
    None,
    Ready(BoxedPulsarProducer),
    Sending(BoxFuture<'static, (BoxedPulsarProducer, Result<SendFuture, PulsarError>)>),
}

struct PulsarSink {
    encoding: EncodingConfig<Encoding>,
    avro_schema: Option<avro_rs::Schema>,
    state: PulsarSinkState,
    in_flight:
        FuturesUnordered<BoxFuture<'static, (usize, Result<CommandSendReceipt, PulsarError>)>>,

    acker: Acker,
    seq_head: usize,
    seq_tail: usize,
    pending_acks: HashSet<usize>,
}

inventory::submit! {
    SinkDescription::new::<PulsarSinkConfig>("pulsar")
}

impl GenerateConfig for PulsarSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            endpoint: "pulsar://127.0.0.1:6650".to_string(),
            topic: "topic-1234".to_string(),
            encoding: Encoding::Text.into(),
            auth: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "pulsar")]
impl SinkConfig for PulsarSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let producer = self
            .create_pulsar_producer()
            .await
            .context(CreatePulsarSinkSnafu)?;
        let sink = PulsarSink::new(producer, self.encoding.clone(), cx.acker())?;

        let producer = self
            .create_pulsar_producer()
            .await
            .context(CreatePulsarSinkSnafu)?;
        let healthcheck = healthcheck(producer).boxed();

        Ok((super::VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "pulsar"
    }
}

impl PulsarSinkConfig {
    async fn create_pulsar_producer(&self) -> Result<PulsarProducer, PulsarError> {
        let mut builder = Pulsar::builder(&self.endpoint, TokioExecutor);
        if let Some(auth) = &self.auth {
            builder = builder.with_auth(Authentication {
                name: auth.name.clone(),
                data: auth.token.as_bytes().to_vec(),
            });
        }

        if let Some(avro_schema) = &self.encoding.schema() {
            let pulsar = builder.build().await?;
            pulsar
                .producer()
                .with_options(pulsar::producer::ProducerOptions {
                    schema: Some(proto::Schema {
                        schema_data: avro_schema.to_string().into_bytes(),
                        r#type: proto::schema::Type::Avro as i32,
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .with_topic(&self.topic)
                .build()
                .await
        } else {
            let pulsar = builder.build().await?;
            pulsar.producer().with_topic(&self.topic).build().await
        }
    }
}

async fn healthcheck(producer: PulsarProducer) -> crate::Result<()> {
    producer.check_connection().await.map_err(Into::into)
}

impl PulsarSink {
    fn new(
        producer: PulsarProducer,
        encoding: EncodingConfig<Encoding>,
        acker: Acker,
    ) -> crate::Result<Self> {
        let schema = match &encoding.codec() {
            Encoding::Avro => {
                if let Some(schema) = &encoding.schema() {
                    avro_rs::Schema::parse_str(schema).ok()
                } else {
                    return Err(
                        "Avro requires a schema, specify a schema file with `encoding.schema`."
                            .into(),
                    );
                }
            }
            _ => None,
        };

        Ok(Self {
            encoding,
            avro_schema: schema,
            state: PulsarSinkState::Ready(Box::new(producer)),
            in_flight: FuturesUnordered::new(),
            acker,
            seq_head: 0,
            seq_tail: 0,
            pending_acks: HashSet::new(),
        })
    }

    fn poll_in_flight_prepare(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if let PulsarSinkState::Sending(fut) = &mut self.state {
            let (producer, result) = ready!(fut.as_mut().poll(cx));

            let seqno = self.seq_head;
            self.seq_head += 1;

            self.state = PulsarSinkState::Ready(producer);
            self.in_flight.push(Box::pin(async move {
                let result = match result {
                    Ok(fut) => fut.await,
                    Err(error) => Err(error),
                };
                (seqno, result)
            }));
        }

        Poll::Ready(())
    }
}

impl Sink<Event> for PulsarSink {
    type Error = ();

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.poll_in_flight_prepare(cx));
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Event) -> Result<(), Self::Error> {
        assert!(
            matches!(self.state, PulsarSinkState::Ready(_)),
            "Expected `poll_ready` to be called first."
        );

        let message = encode_event(item, &self.encoding, &self.avro_schema).map_err(|e| {
            emit!(&PulsarEncodeEventFailed {
                error: &*e.to_string()
            })
        })?;

        let mut producer = match std::mem::replace(&mut self.state, PulsarSinkState::None) {
            PulsarSinkState::Ready(producer) => producer,
            _ => unreachable!(),
        };

        let _ = std::mem::replace(
            &mut self.state,
            PulsarSinkState::Sending(Box::pin(async move {
                let result = producer.send(message).await;
                (producer, result)
            })),
        );

        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.poll_in_flight_prepare(cx));

        let this = Pin::into_inner(self);
        while !this.in_flight.is_empty() {
            match ready!(Pin::new(&mut this.in_flight).poll_next(cx)) {
                Some((seqno, Ok(result))) => {
                    trace!(
                        message = "Pulsar sink produced message.",
                        message_id = ?result.message_id,
                        producer_id = %result.producer_id,
                        sequence_id = %result.sequence_id,
                    );

                    this.pending_acks.insert(seqno);

                    let mut num_to_ack = 0;
                    while this.pending_acks.remove(&this.seq_tail) {
                        num_to_ack += 1;
                        this.seq_tail += 1
                    }
                    this.acker.ack(num_to_ack);
                }
                Some((_, Err(error))) => {
                    error!(message = "Pulsar sink generated an error.", %error);
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

fn encode_event(
    mut item: Event,
    encoding: &EncodingConfig<Encoding>,
    avro_schema: &Option<avro_rs::Schema>,
) -> crate::Result<Vec<u8>> {
    encoding.apply_rules(&mut item);
    let log = item.into_log();

    Ok(match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&log)?,
        Encoding::Text => log
            .get(log_schema().message_key())
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
        Encoding::Avro => {
            let value = avro_rs::to_value(log)?;
            let resolved_value =
                avro_rs::types::Value::resolve(value, avro_schema.as_ref().unwrap())?;
            avro_rs::to_avro_datum(
                avro_schema
                    .as_ref()
                    .expect("Avro encoding selected but no schema found. Please report this."),
                resolved_value,
            )?
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PulsarSinkConfig>();
    }

    #[test]
    fn pulsar_event_json() {
        let msg = "hello_world".to_owned();
        let mut evt = Event::from(msg.clone());
        evt.as_mut_log().insert("key", "value");
        let result = encode_event(evt, &EncodingConfig::from(Encoding::Json), &None).unwrap();
        let map: HashMap<String, String> = serde_json::from_slice(&result[..]).unwrap();
        assert_eq!(msg, map[&log_schema().message_key().to_string()]);
    }

    #[test]
    fn pulsar_event_text() {
        let msg = "hello_world".to_owned();
        let evt = Event::from(msg.clone());
        let event = encode_event(evt, &EncodingConfig::from(Encoding::Text), &None).unwrap();

        assert_eq!(&event[..], msg.as_bytes());
    }

    #[test]
    fn pulsar_event_avro() {
        let raw_schema = r#"
        {
          "type": "record",
          "name": "Log",
          "fields": [
            {"name": "message","type": ["null","string"]}
          ]
        }
        "#;

        let msg = "hello_world".to_owned();
        let mut evt = Event::from(msg);
        evt.as_mut_log().insert("key", "value");
        let mut encoding = EncodingConfig::from(Encoding::Avro);
        encoding.schema = Some(raw_schema.to_string());
        let schema = avro_rs::Schema::parse_str(raw_schema).unwrap();
        let result = encode_event(evt.clone(), &encoding, &Some(schema.clone())).unwrap();

        let value = avro_rs::to_value(evt.into_log()).unwrap();
        let resolved_value = avro_rs::types::Value::resolve(value, &schema).unwrap();
        let must_be = avro_rs::to_avro_datum(&schema, resolved_value).unwrap();

        assert_eq!(result, must_be);
    }

    #[test]
    fn pulsar_encode_event() {
        let msg = "hello_world";

        let mut evt = Event::from(msg);
        evt.as_mut_log().insert("key", "value");

        let event = encode_event(
            evt,
            &EncodingConfig {
                codec: Encoding::Json,
                schema: None,
                only_fields: None,
                except_fields: Some(vec!["key".into()]),
                timestamp_format: None,
            },
            &None,
        )
        .unwrap();

        let map: HashMap<String, String> = serde_json::from_slice(&event[..]).unwrap();
        assert!(!map.contains_key("key"));
    }
}

#[cfg(feature = "pulsar-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use futures::StreamExt;
    use pulsar::SubType;

    use super::*;
    use crate::sinks::VectorSink;
    use crate::test_util::{random_lines_with_stream, random_string, trace_init};

    fn pulsar_address() -> String {
        std::env::var("PULSAR_ADDRESS").unwrap_or_else(|_| "pulsar://127.0.0.1:6650".into())
    }

    #[tokio::test]
    async fn pulsar_happy() {
        trace_init();

        let num_events = 1_000;
        let (_input, events) = random_lines_with_stream(100, num_events, None);

        let topic = format!("test-{}", random_string(10));
        let cnf = PulsarSinkConfig {
            endpoint: pulsar_address(),
            topic: topic.clone(),
            encoding: Encoding::Text.into(),
            auth: None,
        };

        let pulsar = Pulsar::<TokioExecutor>::builder(&cnf.endpoint, TokioExecutor)
            .build()
            .await
            .unwrap();
        let mut consumer = pulsar
            .consumer()
            .with_topic(&topic)
            .with_consumer_name("VectorTestConsumer")
            .with_subscription_type(SubType::Shared)
            .with_subscription("VectorTestSub")
            .with_options(pulsar::consumer::ConsumerOptions {
                read_compacted: Some(false),
                ..Default::default()
            })
            .build::<String>()
            .await
            .unwrap();

        let (acker, ack_counter) = Acker::basic();
        let producer = cnf.create_pulsar_producer().await.unwrap();
        let sink = PulsarSink::new(producer, cnf.encoding, acker).unwrap();
        let sink = VectorSink::from_event_sink(sink);
        sink.run(events).await.unwrap();

        assert_eq!(
            ack_counter.load(std::sync::atomic::Ordering::Relaxed),
            num_events
        );

        for _ in 0..num_events {
            let msg = match consumer.next().await.unwrap() {
                Ok(msg) => msg,
                Err(error) => panic!("{:?}", error),
            };
            consumer.ack(&msg).await.unwrap();
        }
    }
}
