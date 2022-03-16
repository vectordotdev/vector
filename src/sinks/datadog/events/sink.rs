use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use tower::Service;
use vector_buffers::Acker;
use vector_core::stream::DriverResponse;

use crate::{
    config::log_schema,
    event::{Event, LogEvent},
    internal_events::ParserMissingFieldError,
    sinks::{
        datadog::events::request_builder::{DatadogEventsRequest, DatadogEventsRequestBuilder},
        util::{SinkBuilderExt, StreamSink},
    },
};

pub struct DatadogEventsSink<S> {
    pub(super) service: S,
    pub acker: Acker,
}

impl<S> DatadogEventsSink<S>
where
    S: Service<DatadogEventsRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let concurrency_limit = NonZeroUsize::new(50);

        let driver = input
            .map(|event| {
                // Panic: This sink only accepts Logs, so this should never panic
                event.into_log()
            })
            .filter_map(ensure_required_fields)
            .request_builder(concurrency_limit, DatadogEventsRequestBuilder::new())
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build DatadogEvents request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.acker);
        driver.run().await
    }
}

async fn ensure_required_fields(mut log: LogEvent) -> Option<LogEvent> {
    if !log.contains("title") {
        emit!(&ParserMissingFieldError { field: "title" });
        return None;
    }

    let log_schema = log_schema();

    if !log.contains("text") {
        if let Some(message) = log.remove(log_schema.message_key()) {
            log.insert("text", message);
        } else {
            emit!(&ParserMissingFieldError {
                field: log_schema.message_key()
            });
            return None;
        }
    }

    if !log.contains("host") {
        if let Some(host) = log.remove(log_schema.host_key()) {
            log.insert("host", host);
        }
    }

    if !log.contains("date_happened") {
        if let Some(timestamp) = log.remove(log_schema.timestamp_key()) {
            log.insert("date_happened", timestamp);
        }
    }

    if !log.contains("source_type_name") {
        if let Some(name) = log.remove(log_schema.source_type_key()) {
            log.insert("source_type_name", name);
        }
    }
    Some(log)
}

#[async_trait]
impl<S> StreamSink<Event> for DatadogEventsSink<S>
where
    S: Service<DatadogEventsRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run(input).await
    }
}
