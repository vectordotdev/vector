use rdkafka::producer::{FutureProducer, FutureRecord};
use crate::kafka::KafkaStatisticsContext;
use tower::Service;
use std::task::{Context, Poll};
use futures::future::BoxFuture;
use rdkafka::message::OwnedHeaders;
use rdkafka::util::Timeout;
use rdkafka::error::KafkaError;
use crate::buffers::Ackable;
use crate::event::{Finalizable, EventFinalizers, EventStatus};
use std::sync::atomic::{AtomicU64};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::time::Duration;
use std::future::Future;
use std::pin::Pin;
use tokio::time::Sleep;
use crate::sinks::kafka::config::QUEUED_MIN_MESSAGES;

pub struct KafkaRequest {
    pub topic: String,
    pub body: Vec<u8>,
    pub metadata: KafkaRequestMetadata
}

pub struct KafkaRequestMetadata {
    pub finalizers: EventFinalizers,
    pub key: Option<Vec<u8>>,
    pub timestamp_millis: Option<i64>,
    pub headers: Option<OwnedHeaders>,
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

// #[derive(Clone)]
pub struct KafkaService {
    kafka_producer: FutureProducer<KafkaStatisticsContext>,
    current_in_flight: Arc<AtomicU64>,
    delay: Option<Pin<Box<Sleep>>>
}

impl KafkaService {
    pub fn new(kafka_producer: FutureProducer<KafkaStatisticsContext>) -> KafkaService {
        KafkaService {
            kafka_producer,
            current_in_flight: Arc::new(AtomicU64::new(0)),
            delay: None,
        }
    }
}

impl Service<KafkaRequest> for KafkaService {
    type Response = KafkaResponse;
    type Error = KafkaError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // rdkafka has an `in_flight_count()` but that can't be used here
        // since it doesn't count requests that have not been queued yet
        if self.current_in_flight.load(Ordering::Relaxed) < QUEUED_MIN_MESSAGES {
            return Poll::Ready(Ok(()));
        }

        // This is the same amount of time that rdkafka delays when the internal queue is full
        let mut sleep = Box::pin(tokio::time::sleep(Duration::from_millis(100)));
        let result = sleep.as_mut().poll(cx).map(|_|Ok(()));
        self.delay = Some(sleep);// prevent the timer from being dropped
        result
    }

    fn call(&mut self, request: KafkaRequest) -> Self::Future {
        self.current_in_flight.fetch_add(1, Ordering::Relaxed);
        let kafka_producer = self.kafka_producer.clone();
        let current_in_flight = Arc::clone(&self.current_in_flight);

        Box::pin(async move {
            let mut record = FutureRecord::to(&request.topic)
                .payload(&request.body);
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
                Ok((_partition, _offset)) => Ok(KafkaResponse{}),
                Err((kafka_err, _original_record)) => Err(kafka_err)
            };
            current_in_flight.fetch_sub(1, Ordering::Relaxed);
            result
        })
    }
}
