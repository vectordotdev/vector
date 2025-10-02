//! Sentry sink
//!
//! This sink sends events to Sentry using the envelope API endpoint.

use crate::{
    event::Event,
    sinks::{
        prelude::*,
        util::http::{HttpJsonBatchSizer, HttpRequest},
    },
};

use super::request_builder::SentryRequestBuilder;

pub(super) struct SentrySink<S> {
    service: S,
    request_settings: SentryRequestBuilder,
    batch_settings: BatcherSettings,
}

impl<S> SentrySink<S>
where
    S: Service<HttpRequest<()>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    pub(super) fn new(
        service: S,
        batch_settings: BatcherSettings,
        request_settings: SentryRequestBuilder,
    ) -> Result<Self, crate::Error> {
        Ok(Self {
            service,
            request_settings,
            batch_settings,
        })
    }
}

#[async_trait]
impl<S> StreamSink<Event> for SentrySink<S>
where
    S: Service<HttpRequest<()>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

impl<S> SentrySink<S>
where
    S: Service<HttpRequest<()>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            // Batch the input stream with size calculation based on the estimated encoded json size
            .batched(self.batch_settings.as_item_size_config(HttpJsonBatchSizer))
            // Build requests with default concurrency limit.
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_settings,
            )
            // Filter out any errors that occurred in the request building.
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
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
