use aws_sdk_sqs::operation::send_message_batch::SendMessageBatchError;
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use tower::Service;
use vector_lib::stream::BatcherSettings;

use super::client::{BatchSendMessageRequest, SqsBatchMessagePublisher};
use super::{SSRequestBuilder, SendMessageEntry};
use crate::sinks::prelude::*;

/// Batched SQS sink using the send_message_batch API
#[derive(Clone)]
pub(super) struct BatchedSqsSink {
    pub(super) batch_settings: BatcherSettings,
    pub(super) request_builder: SSRequestBuilder,
    pub(super) service: BatchSqsService,
}

impl BatchedSqsSink {
    pub(super) fn new(
        batch_settings: BatcherSettings,
        request_builder: SSRequestBuilder,
        _request: TowerRequestConfig,
        publisher: SqsBatchMessagePublisher,
    ) -> crate::Result<Self> {
        Ok(BatchedSqsSink {
            batch_settings,
            request_builder,
            service: BatchSqsService::new(publisher),
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let service = self.service;

        input
            // Build individual requests from events
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|req| async move {
                req.map_err(|error| {
                    emit!(SinkRequestBuildError { error });
                })
                .ok()
            })
            // Batch the requests
            .batched(self.batch_settings.as_byte_size_config())
            .map(|entries: Vec<SendMessageEntry>| {
                let metadata =
                    RequestMetadata::from_batch(entries.iter().map(|e| e.get_metadata().clone()));
                BatchSendMessageRequest { entries, metadata }
            })
            .into_driver(service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for BatchedSqsSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Service for handling batched SQS requests
#[derive(Clone)]
pub(super) struct BatchSqsService {
    publisher: SqsBatchMessagePublisher,
}

impl BatchSqsService {
    pub(super) const fn new(publisher: SqsBatchMessagePublisher) -> Self {
        Self { publisher }
    }
}

impl Service<BatchSendMessageRequest> for BatchSqsService {
    type Response = super::SendMessageResponse;
    type Error = SdkError<SendMessageBatchError, HttpResponse>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: BatchSendMessageRequest) -> Self::Future {
        let publisher = self.publisher.clone();
        Box::pin(async move { publisher.send_message_batch(request).await })
    }
}
