//! Implementation of the `http` sink.

use crate::sinks::{prelude::*, util::http::HttpRequest};
use std::collections::BTreeMap;

use super::{batch::HttpBatchSizer, request_builder::HttpRequestBuilder};

pub(super) struct HttpSink<S> {
    service: S,
    uri: Template,
    headers: BTreeMap<String, Template>,
    batch_settings: BatcherSettings,
    request_builder: HttpRequestBuilder,
}

impl<S> HttpSink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    /// Creates a new `HttpSink`.
    pub(super) const fn new(
        service: S,
        uri: Template,
        headers: BTreeMap<String, Template>,
        batch_settings: BatcherSettings,
        request_builder: HttpRequestBuilder,
    ) -> Self {
        Self {
            service,
            uri,
            headers,
            batch_settings,
            request_builder,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_sizer = HttpBatchSizer {
            encoder: self.request_builder.encoder.encoder.clone(),
        };
        input
            // Batch the input stream with size calculation based on the configured codec
            .batched_partitioned(KeyPartitioner::new(self.uri, self.headers), || {
                self.batch_settings.as_item_size_config(batch_sizer.clone())
            })
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            // Build requests with default concurrency limit.
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
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

#[async_trait::async_trait]
impl<S> StreamSink<Event> for HttpSink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
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

#[derive(Eq, PartialEq, Clone, Debug, Hash)]
pub struct PartitionKey {
    pub uri: String,
    pub headers: BTreeMap<String, String>,
}

struct KeyPartitioner {
    uri: Template,
    headers: BTreeMap<String, Template>,
}

impl KeyPartitioner {
    const fn new(uri: Template, headers: BTreeMap<String, Template>) -> Self {
        Self { uri, headers }
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = Option<PartitionKey>;

    fn partition(&self, event: &Event) -> Self::Key {
        let uri = self
            .uri
            .render_string(event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("uri"),
                    drop_event: true,
                });
            })
            .ok()?;

        let mut headers = BTreeMap::new();
        for (name, template) in &self.headers {
            let value = template
                .render_string(event)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("headers"),
                        drop_event: true,
                    });
                })
                .ok()?;
            headers.insert(name.clone(), value);
        }

        Some(PartitionKey { uri, headers })
    }
}
