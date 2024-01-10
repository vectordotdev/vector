use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use futures_util::{
    stream::{self, BoxStream},
    StreamExt,
};
use tokio::sync::oneshot::{channel, Sender};
use tower::Service;
use vector_lib::stream::{BatcherSettings, DriverResponse};
use vector_lib::{config::log_schema, event::Event, partition::Partitioner, sink::StreamSink};
use vrl::event_path;
use vrl::path::PathPrefix;

use crate::{
    internal_events::DatadogTracesEncodingError,
    sinks::{datadog::traces::request_builder::DatadogTracesRequestBuilder, util::SinkBuilderExt},
};

use super::service::TraceApiRequest;

#[derive(Default)]
struct EventPartitioner;

// Use all fields from the top level protobuf construct associated with the API key
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) struct PartitionKey {
    pub(crate) api_key: Option<Arc<str>>,
    pub(crate) env: Option<String>,
    pub(crate) hostname: Option<String>,
    pub(crate) agent_version: Option<String>,
    // Those two last fields are configuration value and not a per-trace/span information, they come from the Datadog
    // trace-agent config directly: https://github.com/DataDog/datadog-agent/blob/0f73a78/pkg/trace/config/config.go#L293-L294
    pub(crate) target_tps: Option<i64>,
    pub(crate) error_tps: Option<i64>,
}

impl Partitioner for EventPartitioner {
    type Item = Event;
    type Key = PartitionKey;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        match item {
            Event::Metric(_) => {
                panic!("unexpected metric");
            }
            Event::Log(_) => {
                panic!("unexpected log");
            }
            Event::Trace(t) => PartitionKey {
                api_key: item.metadata().datadog_api_key(),
                env: t
                    .get(event_path!("env"))
                    .map(|s| s.to_string_lossy().into_owned()),
                hostname: log_schema().host_key().and_then(|key| {
                    t.get((PathPrefix::Event, key))
                        .map(|s| s.to_string_lossy().into_owned())
                }),
                agent_version: t
                    .get(event_path!("agent_version"))
                    .map(|s| s.to_string_lossy().into_owned()),
                target_tps: t
                    .get(event_path!("target_tps"))
                    .and_then(|tps| tps.as_integer().map(Into::into)),
                error_tps: t
                    .get(event_path!("error_tps"))
                    .and_then(|tps| tps.as_integer().map(Into::into)),
            },
        }
    }
}

pub struct TracesSink<S> {
    service: S,
    request_builder: DatadogTracesRequestBuilder,
    batch_settings: BatcherSettings,
    shutdown: Sender<Sender<()>>,
    protocol: String,
}

impl<S> TracesSink<S>
where
    S: Service<TraceApiRequest> + Send,
    S::Error: Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    pub const fn new(
        service: S,
        request_builder: DatadogTracesRequestBuilder,
        batch_settings: BatcherSettings,
        shutdown: Sender<Sender<()>>,
        protocol: String,
    ) -> Self {
        TracesSink {
            service,
            request_builder,
            batch_settings,
            shutdown,
            protocol,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;

        input
            .batched_partitioned(EventPartitioner, || batch_settings.as_byte_size_config())
            .incremental_request_builder(self.request_builder)
            .flat_map(stream::iter)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        let (error_message, error_reason, dropped_events) = e.into_parts();
                        emit!(DatadogTracesEncodingError {
                            error_message,
                            error_reason,
                            dropped_events: dropped_events as usize,
                        });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .protocol(self.protocol)
            .run()
            .await?;

        // Create a channel for the stats flushing thread to communicate back that it has flushed
        // remaining stats. This is necessary so that we do not terminate the process while the
        // stats flushing thread is trying to complete the HTTP request.
        let (sender, receiver) = channel();

        // Signal the stats thread task to flush remaining payloads and shutdown.
        _ = self.shutdown.send(sender);

        // The stats flushing thread has until the component shutdown grace period to end
        // gracefully. Otherwise the sink + stats flushing thread will be killed and an error
        // reported upstream.
        receiver.await.map_err(|_| ())
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
