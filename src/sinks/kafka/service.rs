use crate::buffers::Ackable;
use crate::event::{EventFinalizers, EventStatus, Finalizable};
use crate::kafka::KafkaStatisticsContext;
use futures::future::BoxFuture;
use rdkafka::error::KafkaError;
use rdkafka::message::OwnedHeaders;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;
use std::task::{Context, Poll};
use tower::Service;

pub struct KafkaRequest {
    pub body: Vec<u8>,
    pub metadata: KafkaRequestMetadata,
}

pub struct KafkaRequestMetadata {
    pub finalizers: EventFinalizers,
    pub key: Option<Vec<u8>>,
    pub timestamp_millis: Option<i64>,
    pub headers: Option<OwnedHeaders>,
    pub topic: String,
}

pub struct KafkaResponse {}

impl AsRef<EventStatus> for KafkaResponse {
    fn as_ref(&self) -> &EventStatus {
        &EventStatus::Delivered
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
                record = record.key(key);
            }
            if let Some(timestamp) = request.metadata.timestamp_millis {
                record = record.timestamp(timestamp);
            }
            if let Some(headers) = request.metadata.headers {
                record = record.headers(headers);
            }

            //rdkafka will internally retry forever if the queue is full
            let result = match kafka_producer.send(record, Timeout::Never).await {
                Ok((_partition, _offset)) => Ok(KafkaResponse {}),
                Err((kafka_err, _original_record)) => Err(kafka_err),
            };
            result
        })
    }
}
