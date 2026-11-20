use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use futures_util::{
    StreamExt,
    stream::{self, BoxStream},
};
use tokio::sync::oneshot::{Sender, channel};
use tower::Service;
#[cfg(feature = "typed-trace")]
use vector_lib::event::EventContainer;
#[cfg(not(feature = "typed-trace"))]
use vector_lib::{config::log_schema, event::Value};
use vector_lib::{
    event::Event,
    internal_event::emit,
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};
#[cfg(not(feature = "typed-trace"))]
use vrl::{event_path, path::PathPrefix};

use super::service::TraceApiRequest;
use crate::{
    internal_events::DatadogTracesEncodingError,
    sinks::{datadog::traces::request_builder::DatadogTracesRequestBuilder, util::SinkBuilderExt},
};

#[cfg(not(feature = "typed-trace"))]
#[derive(Default)]
struct EventPartitioner;

/// Batch key for the typed path. Agent envelope fields are grouped inside the batch.
#[cfg(feature = "typed-trace")]
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) struct PartitionKey {
    pub(crate) api_key: Option<Arc<str>>,
}

// Use all fields from the top level protobuf construct associated with the API key.
#[cfg(not(feature = "typed-trace"))]
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

#[cfg(not(feature = "typed-trace"))]
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
                target_tps: t.get(event_path!("target_tps")).and_then(Value::as_integer),
                error_tps: t.get(event_path!("error_tps")).and_then(Value::as_integer),
            },
        }
    }
}

pub(super) struct TracesSink<S> {
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
    pub(super) const fn new(
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

    #[cfg(not(feature = "typed-trace"))]
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;

        input
            .batched_partitioned(EventPartitioner, batch_settings.timeout, |_| {
                batch_settings.as_byte_size_config()
            })
            .incremental_request_builder(self.request_builder)
            .flat_map(stream::iter)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        let (error_message, error_reason, dropped_events) = e.into_parts();
                        emit(DatadogTracesEncodingError {
                            error_message,
                            error_reason,
                            dropped_events,
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

#[cfg(not(feature = "typed-trace"))]
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

#[cfg(feature = "typed-trace")]
struct TypedPartitioner;

#[cfg(feature = "typed-trace")]
impl Partitioner for TypedPartitioner {
    type Item = crate::event::typed_trace::TraceEvent;
    type Key = PartitionKey;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        PartitionKey {
            api_key: item.metadata().datadog_api_key(),
        }
    }
}

#[cfg(feature = "typed-trace")]
fn convert_array(array: crate::event::EventArray) -> Vec<crate::event::typed_trace::TraceEvent> {
    array.into_events().flat_map(convert_event).collect()
}

#[cfg(feature = "typed-trace")]
fn convert_event(event: Event) -> Vec<crate::event::typed_trace::TraceEvent> {
    let Event::Trace(legacy) = event else {
        panic!("unexpected non-trace event");
    };
    let mut reporter = super::typed::IssueLog;
    match vector_lib::trace_conversion::convert_legacy(&legacy, &mut reporter) {
        Ok(events) => events,
        Err(error) => {
            emit(DatadogTracesEncodingError {
                error_message: "Failed to convert trace event.",
                error_reason: error.to_string(),
                dropped_events: 1,
            });
            let mut legacy = legacy;
            legacy
                .metadata_mut()
                .update_status(vector_lib::event::EventStatus::Rejected);
            Vec::new()
        }
    }
}

#[cfg(feature = "typed-trace")]
impl<S> TracesSink<S>
where
    S: Service<TraceApiRequest> + Send,
    S::Error: Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    async fn run_inner(
        self: Box<Self>,
        input: BoxStream<'_, crate::event::EventArray>,
    ) -> Result<(), ()> {
        let batch_settings = self.batch_settings;

        input
            .map(convert_array)
            .flat_map(stream::iter)
            .batched_partitioned(TypedPartitioner, batch_settings.timeout, |_| {
                batch_settings.as_byte_size_config()
            })
            .incremental_request_builder(self.request_builder)
            .flat_map(stream::iter)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        let (error_message, error_reason, dropped_events) = e.into_parts();
                        emit(DatadogTracesEncodingError {
                            error_message,
                            error_reason,
                            dropped_events,
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

        let (sender, receiver) = channel();
        _ = self.shutdown.send(sender);
        receiver.await.map_err(|_| ())
    }
}

#[cfg(feature = "typed-trace")]
#[async_trait]
impl<S> StreamSink<crate::event::EventArray> for TracesSink<S>
where
    S: Service<TraceApiRequest> + Send,
    S::Error: Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    async fn run(
        self: Box<Self>,
        input: BoxStream<'_, crate::event::EventArray>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
