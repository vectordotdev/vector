use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use futures_util::{
    stream::{self, BoxStream},
    StreamExt,
};
use tower::Service;
use vector_core::{
    buffers::Acker,
    config::log_schema,
    event::Event,
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};

use super::service::TraceApiRequest;
use crate::{
    config::SinkContext,
    sinks::{
        datadog::traces::{
            config::DatadogTracesEndpoint, request_builder::DatadogTracesRequestBuilder,
        },
        util::SinkBuilderExt,
    },
};
#[derive(Default)]
struct EventPartitioner;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) struct PartitionKey {
    pub(crate) api_key: Option<Arc<str>>,
    pub(crate) env: Option<String>,
    pub(crate) hostname: Option<String>,
    pub(crate) lang: Option<String>,
    pub(crate) endpoint: DatadogTracesEndpoint,
}

impl Partitioner for EventPartitioner {
    type Item = Event;
    type Key = PartitionKey;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let (endpoint, env, hostname, lang) = match item {
            Event::Metric(_) => (DatadogTracesEndpoint::APMStats, None, None, None),
            Event::Log(_) => {
                panic!("unexpected log");
            }
            Event::Trace(t) => (
                DatadogTracesEndpoint::Traces,
                t.get("env").map(|s| s.to_string_lossy()),
                t.get(log_schema().host_key()).map(|s| s.to_string_lossy()),
                t.get("language").map(|s| s.to_string_lossy()),
            ),
        };

        PartitionKey {
            api_key: item.metadata().datadog_api_key().clone(),
            env,
            hostname,
            lang,
            endpoint,
        }
    }
}

pub struct TracesSink<S> {
    service: S,
    acker: Acker,
    request_builder: DatadogTracesRequestBuilder,
    batch_settings: BatcherSettings,
}

impl<S> TracesSink<S>
where
    S: Service<TraceApiRequest> + Send,
    S::Error: Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    pub fn new(
        cx: SinkContext,
        service: S,
        request_builder: DatadogTracesRequestBuilder,
        batch_settings: BatcherSettings,
    ) -> Self {
        TracesSink {
            service,
            acker: cx.acker(),
            request_builder,
            batch_settings,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let sink = input
            .batched_partitioned(EventPartitioner, self.batch_settings)
            .incremental_request_builder(self.request_builder)
            .flat_map(stream::iter)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        let (error, _dropped_events) = e.into_parts();
                        error!("Failed to build Datadog Traces request: {:?}.", error);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for TracesSink<S>
where
    S: Service<TraceApiRequest> + Send,
    S::Error: Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
