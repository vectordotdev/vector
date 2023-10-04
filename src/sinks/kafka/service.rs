use std::{
    sync::{
        atomic::{AtomicI64, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use bytes::Bytes;
use rdkafka::{
    error::KafkaError,
    message::OwnedHeaders,
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};

use crate::{kafka::KafkaStatisticsContext, sinks::prelude::*};

pub struct KafkaRequest {
    pub body: Bytes,
    pub metadata: KafkaRequestMetadata,
    pub request_metadata: RequestMetadata,
}

pub struct KafkaRequestMetadata {
    pub finalizers: EventFinalizers,
    pub key: Option<Bytes>,
    pub timestamp_millis: Option<i64>,
    pub headers: Option<OwnedHeaders>,
    pub topic: String,
}

pub struct KafkaResponse {
    event_byte_size: GroupedCountByteSize,
    raw_byte_size: usize,
}

impl DriverResponse for KafkaResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.event_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.raw_byte_size)
    }
}

impl Finalizable for KafkaRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl MetaDescriptive for KafkaRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

#[derive(Clone)]
pub struct KafkaService {
    kafka_producer: FutureProducer<KafkaStatisticsContext>,

    // State to keep track of the producer queue's current size and limits. We use i64 here instead
    // of i32 to avoid any potential overflow bugs that might result from race conditions.
    queue_messages_max: i64,
    queue_bytes_max: i64,
    queue_messages_current: Arc<AtomicI64>,
    queue_bytes_current: Arc<AtomicI64>,
}

impl KafkaService {
    pub(crate) const fn new(
        kafka_producer: FutureProducer<KafkaStatisticsContext>,
        queue_messages_max: i32,
        queue_bytes_max: i32,
    ) -> KafkaService {
        KafkaService {
            kafka_producer,
            queue_messages_max: queue_messages_max as i64,
            queue_bytes_max: queue_bytes_max as i64,
            queue_messages_current: Arc::new(AtomicI64::new(0)),
            queue_bytes_current: Arc::new(AtomicI64::new(0)),
        }
    }
}

impl Service<KafkaRequest> for KafkaService {
    type Response = KafkaResponse;
    type Error = KafkaError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // The Kafka service is available if its producer queue is not full.
        if self.queue_messages_current.load(Ordering::Relaxed) < self.queue_messages_max
            && self.queue_bytes_current.load(Ordering::Relaxed) < self.queue_bytes_max
        {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    fn call(&mut self, request: KafkaRequest) -> Self::Future {
        let this = self.clone();

        // Update state for the producer queue.
        let raw_byte_size = request.body.len() + request.metadata.key.map_or(0, |x| x.len());
        this.queue_messages_current.fetch_add(1, Ordering::Relaxed);
        this.queue_bytes_current
            .fetch_add(raw_byte_size as i64, Ordering::Relaxed);

        Box::pin(async move {
            let event_byte_size = request
                .request_metadata
                .into_events_estimated_json_encoded_byte_size();

            let mut record =
                FutureRecord::to(&request.metadata.topic).payload(request.body.as_ref());
            if let Some(key) = &request.metadata.key {
                record = record.key(&key[..]);
            }
            if let Some(timestamp) = request.metadata.timestamp_millis {
                record = record.timestamp(timestamp);
            }
            if let Some(headers) = request.metadata.headers {
                record = record.headers(headers);
            }

            // rdkafka will internally retry forever if the queue is full
            let res = match this.kafka_producer.send(record, Timeout::Never).await {
                Ok((_partition, _offset)) => Ok(KafkaResponse {
                    event_byte_size,
                    raw_byte_size,
                }),
                Err((kafka_err, _original_record)) => Err(kafka_err),
            };

            // Update state for the producer queue.
            this.queue_messages_current.fetch_sub(1, Ordering::Relaxed);
            this.queue_bytes_current
                .fetch_sub(raw_byte_size as i64, Ordering::Relaxed);

            res
        })
    }
}
