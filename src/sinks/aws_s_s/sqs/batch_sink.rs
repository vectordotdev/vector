use aws_sdk_sqs::operation::send_message_batch::SendMessageBatchError;
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use tower::Service;
use vector_lib::stream::BatcherSettings;

use super::client::{BatchSendMessageRequest, SqsBatchMessagePublisher};
use super::{SSRequestBuilder, SendMessageEntry};
use crate::sinks::prelude::*;

/// Batched SQS sink using the send_message_batch API.
///
/// ## All-or-Nothing Retry Semantics
///
/// This sink implements **all-or-nothing** retry semantics for batch failures:
/// - **Success**: All messages in the batch succeeded → acknowledge and move to next batch
/// - **Failure**: Any message in the batch failed to send → return error and retry **entire batch**
///
/// When the Publisher detects any failed message (via `response.failed()`), it logs the failures
/// and returns an `Err`, which causes Vector's built-in retry framework to retry the entire batch.
/// This is simpler and more robust than per-message retry because:
/// 1. SQS batches are limited to 10 messages—very low cost to retry all
/// 2. No need to maintain per-message state or index tracking
/// 3. Leverages Vector's deduplication and acknowledgement mechanisms at the request level
/// 4. Matches SQS's atomic batch semantics (succeed or fail together)
///
/// ## Example
///
/// If a batch of 10 messages has 1 failure:
/// 1. Publisher logs the failure: `error!("Message failed in batch (batch will retry)")`
/// 2. Publisher returns `Err(SdkError::service_error(...))`
/// 3. Vector's retry framework catches the error
/// 4. Entire batch of 10 messages is re-queued and retried
/// 5. No message is lost; no duplicates unless Vector's deduplication is bypassed
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
