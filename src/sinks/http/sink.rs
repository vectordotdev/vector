//! Implementation of the `http` sink.

use crate::sinks::prelude::*;

use super::{
    request_builder::HttpRequestBuilder,
    service::{HttpRetryLogic, HttpService},
};

pub(super) struct HttpSink {
    service: Svc<HttpService, HttpRetryLogic>,
    batch_settings: BatcherSettings,
    request_builder: HttpRequestBuilder,
}

impl HttpSink {
    /// Creates a new `HttpSink`.
    pub(super) fn new(
        service: Svc<HttpService, HttpRetryLogic>,
        batch_settings: BatcherSettings,
        request_builder: HttpRequestBuilder,
    ) -> Self {
        Self {
            service,
            batch_settings,
            request_builder,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            // Batch the input stream with byte size calculation.
            .batched(self.batch_settings.into_byte_size_config())
            // Build requests with no concurrency limit.
            .request_builder(None, self.request_builder)
            // Filter out any errors that occured in the request building.
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

#[async_trait::async_trait]
impl StreamSink<Event> for HttpSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
