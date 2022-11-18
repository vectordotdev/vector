use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future::BoxFuture;
use lru::LruCache;
use pulsar::{Error as PulsarError, Executor, Producer, ProducerOptions, Pulsar};
use tokio::sync::Mutex;
use tower::Service;
use vector_common::internal_event::{CountByteSize};
use vector_core::{
    internal_event::{
        BytesSent, Protocol, Registered,
    },
    stream::DriverResponse,
};

use crate::event::{EventFinalizers, EventStatus, Finalizable};
use crate::internal_events::PulsarSendingError;
use std::num::NonZeroUsize;
use vector_common::request_metadata::{MetaDescriptive, RequestMetadata};
use crate::sinks::pulsar::request_builder::PulsarMetadata;
use crate::sinks::util::retries::RetryLogic;

#[derive(Clone)]
pub struct PulsarRequest {
    pub body: Bytes,
    pub metadata: PulsarMetadata,
    pub request_metadata: RequestMetadata
}

pub struct PulsarResponse {
    event_byte_size: usize,
}

impl DriverResponse for PulsarResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(1, self.event_byte_size)
    }
}

impl Finalizable for PulsarRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl MetaDescriptive for PulsarRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.request_metadata
    }
}

/// Pulsar retry logic.
#[derive(Debug, Default, Clone)]
pub struct PulsarRetryLogic;

impl RetryLogic for PulsarRetryLogic {
    type Error = PulsarError;
    type Response = PulsarResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        // TODO improve retry logic
        true
    }
}

type SafeLru<Exe> = Arc<Mutex<LruCache<String, Result<Arc<Mutex<Producer<Exe>>>, PulsarError>>>>;
pub struct PulsarService<Exe: Executor> {
    pulsar_client: Pulsar<Exe>,
    producer_cache: SafeLru<Exe>,
    producer_options: ProducerOptions,
    bytes_sent: Registered<BytesSent>,
}

impl<Exe: Executor> PulsarService<Exe> {
    pub(crate) fn new(
        pulsar_client: Pulsar<Exe>,
        producer_options: ProducerOptions,
        producer_cache_size: Option<NonZeroUsize>,
    ) -> PulsarService<Exe> {
        // Use a LRUCache to store a limited set of producers
        // Producers in Pulsar use a send buffer, so we want to limit the number of these
        let producer_cache = Arc::new(Mutex::new(LruCache::new(
            producer_cache_size.unwrap_or(NonZeroUsize::new(100).unwrap()),
        )));
        PulsarService {
            pulsar_client,
            producer_cache,
            producer_options,
            bytes_sent: register!(BytesSent::from(Protocol("pulsar".into()))),
        }
    }

    /// Build a producer that is wrapped in an Arc<Mutex> to allow for the producer
    /// to control access.
    ///
    /// NOTE: Pulsar client library should likely be improved to simplify this
    async fn build_producer(
        client: Pulsar<Exe>,
        producer_options: ProducerOptions,
        topic: &String,
    ) -> Result<Arc<Mutex<Producer<Exe>>>, PulsarError> {
        let prod = client
            .producer()
            .with_topic(topic)
            .with_options(producer_options);
        match prod.build().await {
            Ok(p) => Ok(Arc::new(Mutex::new(p))),
            Err(e) => Err(e),
        }
    }

    /// Pulsar requires a producer object be created per topic
    /// This method will build a producer if it hasn't been created or caches it otherwise
    async fn get_or_build_producer(
        producer_cache: SafeLru<Exe>,
        client: Pulsar<Exe>,
        producer_options: ProducerOptions,
        topic: String,
    ) -> Arc<Mutex<Producer<Exe>>> {
        let mut pc = producer_cache.lock().await;
        match pc.contains(&topic) {
            false => {
                pc.put(
                    topic.clone(),
                    PulsarService::build_producer(client, producer_options, &topic).await,
                );
                let f = pc.get(&topic).unwrap().as_ref().unwrap();
                Arc::clone(f)
            }
            true => {
                let f = pc.get(&topic).unwrap().as_ref().unwrap();
                Arc::clone(f)
            }
        }
    }
}

impl<Exe: Executor> Service<PulsarRequest> for PulsarService<Exe> {
    type Response = PulsarResponse;
    type Error = PulsarError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: PulsarRequest) -> Self::Future {
        let prod_future = PulsarService::get_or_build_producer(
            Arc::clone(&self.producer_cache),
            self.pulsar_client.clone(),
            self.producer_options.clone(),
            request.metadata.topic.clone(),
        );
        let ts = request.metadata.timestamp_millis.to_owned();
        Box::pin(async move {
            let p = prod_future.await;
            let mut lp = p.lock().await;
            let body = request.body.clone();
            let mut msg_builder = lp.create_message().with_content(body.as_ref());
            if let Some(key) = request.metadata.key {
                msg_builder = msg_builder.with_key(String::from_utf8_lossy(&*key));
            }
            if let Some(timestamp) = ts {
                msg_builder = msg_builder.event_time(timestamp as u64);
            }
            if let Some(properties) = request.metadata.properties {
                for (key, value) in properties {
                    msg_builder =
                        msg_builder.with_property(key, String::from_utf8_lossy(&*value.clone()));
                }
            }

            match msg_builder.send().await {
                Ok(resp) => match resp.await {
                    Ok(_) => {
                        Ok(PulsarResponse {
                            event_byte_size: request.request_metadata.events_byte_size(),
                        })
                    }
                    Err(e) => {
                        emit!(PulsarSendingError {
                            error: Box::new(PulsarError::Custom("failed to send".to_string())),
                            count: 1
                        });
                        Err(e)
                    }
                },
                Err(e) => {
                    emit!(PulsarSendingError {
                        error: Box::new(PulsarError::Custom("failed to send".to_string())),
                        count: 1,
                    });
                    Err(e)
                }
            }
        })
    }
}
