use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use lookup::event_path;
use tower::Service;
use vector_core::stream::DriverResponse;

use crate::{
    event::Event,
    internal_events::{ParserMissingFieldError, SinkRequestBuildError, DROP_EVENT},
    sinks::{
        datadog::events::request_builder::{DatadogEventsRequest, DatadogEventsRequestBuilder},
        util::{SinkBuilderExt, StreamSink},
    },
};

pub struct DatadogEventsSink<S> {
    pub(super) service: S,
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

        input
            .filter_map(ensure_required_fields)
            .request_builder(concurrency_limit, DatadogEventsRequestBuilder::new())
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

async fn ensure_required_fields(event: Event) -> Option<Event> {
    let mut log = event.into_log();

    if !log.contains("title") {
        emit!(ParserMissingFieldError::<DROP_EVENT> { field: "title" });
        return None;
    }

    if !log.contains("text") {
        let message_path = log
            .message_path()
            .expect("message is required (make sure the \"message\" semantic meaning is set)");
        log.rename_key(message_path.as_str(), event_path!("text"))
    }

    if !log.contains("host") {
        if let Some(host_path) = log.host_path() {
            log.rename_key(host_path.as_str(), event_path!("host"));
        }
    }

    if !log.contains("date_happened") {
        if let Some(timestamp_path) = log.timestamp_path() {
            log.rename_key(timestamp_path.as_str(), "date_happened");
        }
    }

    if !log.contains("source_type_name") {
        log.rename_key(log.source_type_path(), "source_type_name")
    }

    Some(Event::from(log))
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
