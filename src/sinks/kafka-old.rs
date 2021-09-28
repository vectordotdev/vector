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

}
