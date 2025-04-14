//! Implementation of the `opentelemetry` sink.

use crate::sinks::{prelude::*, util::http::HttpRequest};

use super::request_builder::OpentelemetryRequestBuilder;

pub(super) struct OpentelemetrySink<S> {
    service: S,
    batch_settings: BatcherSettings,
    request_builder: OpentelemetryRequestBuilder,

    log_endpoint: String,
    trace_endpoint: String,
    metric_endpoint: String,
}

impl<S> OpentelemetrySink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    /// Creates a new `OpentelemetrySink`.
    pub(super) const fn new(
        service: S,
        batch_settings: BatcherSettings,
        request_builder: OpentelemetryRequestBuilder,
        log_endpoint: String,
        trace_endpoint: String,
        metric_endpoint: String,
    ) -> Self {
        Self {
            service,
            batch_settings,
            request_builder,

            log_endpoint,
            trace_endpoint,
            metric_endpoint,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .batched_partitioned(
                KeyPartitioner::new(self.log_endpoint, self.trace_endpoint, self.metric_endpoint),
                || self.batch_settings.as_byte_size_config(),
            )
            // Build requests with no concurrency limit.
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
impl<S> StreamSink<Event> for OpentelemetrySink<S>
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

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(super) struct PartitionKey {
    pub endpoint: String,
}

struct KeyPartitioner {
    log_endpoint: String,
    trace_endpoint: String,
    metric_endpoint: String,
}

impl KeyPartitioner {
    pub fn new(log_endpoint: String, trace_endpoint: String, metric_endpoint: String) -> Self {
        Self {
            log_endpoint,
            trace_endpoint,
            metric_endpoint,
        }
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = PartitionKey;

    fn partition(&self, event: &Self::Item) -> Self::Key {
        match event {
            Event::Log(_) => PartitionKey {
                endpoint: self.log_endpoint.clone(),
            },
            Event::Trace(_) => PartitionKey {
                endpoint: self.trace_endpoint.clone(),
            },
            Event::Metric(_) => PartitionKey {
                endpoint: self.metric_endpoint.clone(),
            },
        }
    }
}
