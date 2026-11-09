//! Implementation of the `opsimate` sink.

use super::request_builder::OpsiMateRequestBuilder;
use crate::sinks::{
    prelude::*,
    util::http::{HttpJsonBatchSizer, HttpRequest},
};
use vector_lib::internal_event::emit;

pub(super) struct OpsiMateSink<S> {
    service: S,
    batch_settings: BatcherSettings,
    request_builder: OpsiMateRequestBuilder,
}

impl<S> OpsiMateSink<S>
where
    S: Service<HttpRequest<()>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    /// Creates a new `opsimate` sink.
    pub(super) const fn new(
        service: S,
        batch_settings: BatcherSettings,
        request_builder: OpsiMateRequestBuilder,
    ) -> Self {
        Self {
            service,
            batch_settings,
            request_builder,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            // Batch the input stream. The batch settings are validated to hold at most one
            // event, so every batch (and therefore every request) carries a single alert.
            .batched(self.batch_settings.as_item_size_config(HttpJsonBatchSizer))
            // Build requests with default concurrency limit.
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            // Filter out any errors that occurred in the request building.
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            // Generate the driver that will send requests and handle retries,
            // event finalization, and logging/internal metric reporting.
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for OpsiMateSink<S>
where
    S: Service<HttpRequest<()>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
