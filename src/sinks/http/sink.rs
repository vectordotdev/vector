//! Implementation of the `http` sink.

use crate::sinks::{
    prelude::*,
    util::http_service::{HttpRetryLogic, HttpService},
};

use super::{
    batch::HttpBatchSizer, request_builder::HttpRequestBuilder, service::HttpSinkRequestBuilder,
};

pub(super) struct HttpSink {
    service: Svc<HttpService<HttpSinkRequestBuilder>, HttpRetryLogic>,
    batch_settings: BatcherSettings,
    request_builder: HttpRequestBuilder,
}

impl HttpSink {
    /// Creates a new `HttpSink`.
    pub(super) const fn new(
        service: Svc<HttpService<HttpSinkRequestBuilder>, HttpRetryLogic>,
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
            // Batch the input stream with size calculation dependent on the configured codec
            .batched(self.batch_settings.into_item_size_config(HttpBatchSizer {
                encoder: self.request_builder.encoder.encoder.clone(),
            }))
            // Build requests with no concurrency limit.
            .request_builder(None, self.request_builder)
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

#[async_trait::async_trait]
impl StreamSink<Event> for HttpSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
