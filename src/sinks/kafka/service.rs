use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future::BoxFuture;
use rdkafka::{
    error::KafkaError,
    message::OwnedHeaders,
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};
use tower::Service;
use vector_core::{
    buffers::Ackable,
    internal_event::{BytesSent, EventsSent},
    stream::DriverResponse,
};

use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    kafka::KafkaStatisticsContext,
};

pub struct KafkaRequest {
    pub body: Vec<u8>,
    pub metadata: KafkaRequestMetadata,
    pub event_byte_size: usize,
}

pub struct KafkaRequestMetadata {
    pub finalizers: EventFinalizers,
    pub key: Option<Bytes>,
    pub timestamp_millis: Option<i64>,
    pub headers: Option<OwnedHeaders>,
    pub topic: String,
}

pub struct KafkaResponse {
    event_byte_size: usize,
}

impl DriverResponse for KafkaResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: 1,
            byte_size: self.event_byte_size,
            output: None,
        }
    }
}

impl Ackable for KafkaRequest {
    fn ack_size(&self) -> usize {
        // rdkafka takes care of batching internally, so a request here is always 1 event
        1
    }
}

impl Finalizable for KafkaRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

pub struct KafkaService {
    kafka_producer: FutureProducer<KafkaStatisticsContext>,
}

impl KafkaService {
    pub const fn new(kafka_producer: FutureProducer<KafkaStatisticsContext>) -> KafkaService {
        KafkaService { kafka_producer }
    }
}

impl Service<KafkaRequest> for KafkaService {
    type Response = KafkaResponse;
    type Error = KafkaError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: KafkaRequest) -> Self::Future {
        let kafka_producer = self.kafka_producer.clone();

        Box::pin(async move {
            let mut record = FutureRecord::to(&request.metadata.topic).payload(&request.body);
            if let Some(key) = &request.metadata.key {
                record = record.key(&key[..]);
            }
            if let Some(timestamp) = request.metadata.timestamp_millis {
                record = record.timestamp(timestamp);
            }
            if let Some(headers) = request.metadata.headers {
                record = record.headers(headers);
            }

            //rdkafka will internally retry forever if the queue is full
            let result = match kafka_producer.send(record, Timeout::Never).await {
                Ok((_partition, _offset)) => {
                    emit!(&BytesSent {
                        byte_size: request.body.len()
                            + request.metadata.key.map(|x| x.len()).unwrap_or(0),
                        protocol: "kafka"
                    });
                    Ok(KafkaResponse {
                        event_byte_size: request.event_byte_size,
                    })
                }
                Err((kafka_err, _original_record)) => Err(kafka_err),
            };
            result
        })
    }
}
