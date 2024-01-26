use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use rdkafka::{
    error::KafkaError,
    message::OwnedHeaders,
    producer::{FutureProducer, FutureRecord},
    types::RDKafkaErrorCode,
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

/// BlockedRecordState manages state for a record blocked from being enqueued on the producer.
struct BlockedRecordState {
    records_blocked: Arc<AtomicUsize>,
}

impl BlockedRecordState {
    fn new(records_blocked: Arc<AtomicUsize>) -> Self {
        records_blocked.fetch_add(1, Ordering::Relaxed);
        Self { records_blocked }
    }
}

impl Drop for BlockedRecordState {
    fn drop(&mut self) {
        self.records_blocked.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub struct KafkaService {
    kafka_producer: FutureProducer<KafkaStatisticsContext>,

    /// The number of records blocked from being enqueued on the producer.
    records_blocked: Arc<AtomicUsize>,
}

impl KafkaService {
    pub(crate) fn new(kafka_producer: FutureProducer<KafkaStatisticsContext>) -> KafkaService {
        KafkaService {
            kafka_producer,
            records_blocked: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Service<KafkaRequest> for KafkaService {
    type Response = KafkaResponse;
    type Error = KafkaError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // The Kafka service is at capacity if any records are currently blocked from being enqueued
        // on the producer.
        if self.records_blocked.load(Ordering::Relaxed) > 0 {
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn call(&mut self, request: KafkaRequest) -> Self::Future {
        let this = self.clone();

        Box::pin(async move {
            let raw_byte_size =
                request.body.len() + request.metadata.key.as_ref().map_or(0, |x| x.len());
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

            // Manually poll [FutureProducer::send_result] instead of [FutureProducer::send] to track
            // records that fail to be enqueued on the producer.
            let mut blocked_state: Option<BlockedRecordState> = None;
            loop {
                match this.kafka_producer.send_result(record) {
                    // Record was successfully enqueued on the producer.
                    Ok(fut) => {
                        // Drop the blocked state (if any), as the producer is no longer blocked.
                        drop(blocked_state.take());
                        return fut
                            .await
                            .expect("producer unexpectedly dropped")
                            .map(|_| KafkaResponse {
                                event_byte_size,
                                raw_byte_size,
                            })
                            .map_err(|(err, _)| err);
                    }
                    // Producer queue is full.
                    Err((
                        KafkaError::MessageProduction(RDKafkaErrorCode::QueueFull),
                        original_record,
                    )) => {
                        if blocked_state.is_none() {
                            blocked_state =
                                Some(BlockedRecordState::new(Arc::clone(&this.records_blocked)));
                        }
                        record = original_record;
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    // A different error occurred.
                    Err((err, _)) => return Err(err),
                };
            }
        })
    }
}
