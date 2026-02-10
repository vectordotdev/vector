//! Implementation of the `azure_data_explorer` sink.

use super::request_builder::AzureDataExplorerRequestBuilder;
use crate::sinks::{
    prelude::*,
    util::http::{HttpJsonBatchSizer, HttpRequest},
};

pub(super) struct AzureDataExplorerSink<S> {
    service: S,
    batch_settings: BatcherSettings,
    request_builder: AzureDataExplorerRequestBuilder,
}

impl<S> AzureDataExplorerSink<S>
where
    S: Service<HttpRequest<()>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    /// Creates a new `AzureDataExplorerSink`.
    pub(super) const fn new(
        service: S,
        batch_settings: BatcherSettings,
        request_builder: AzureDataExplorerRequestBuilder,
    ) -> Self {
        Self {
            service,
            batch_settings,
            request_builder,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .batched(self.batch_settings.as_item_size_config(HttpJsonBatchSizer))
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for AzureDataExplorerSink<S>
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
