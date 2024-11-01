use std::fmt;

use vector_lib::lookup::event_path;

use crate::{
    internal_events::{ParserMissingFieldError, DROP_EVENT},
    sinks::{
        datadog::events::request_builder::{DatadogEventsRequest, DatadogEventsRequestBuilder},
        prelude::*,
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
        input
            .filter_map(ensure_required_fields)
            .request_builder(
                default_request_builder_concurrency_limit(),
                DatadogEventsRequestBuilder::new(),
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

async fn ensure_required_fields(event: Event) -> Option<Event> {
    let mut log = event.into_log();

    if !log.contains(event_path!("title")) {
        emit!(ParserMissingFieldError::<DROP_EVENT> { field: "title" });
        return None;
    }

    if !log.contains(event_path!("text")) {
        let message_path = log
            .message_path()
            .expect("message is required (make sure the \"message\" semantic meaning is set)")
            .clone();
        log.rename_key(&message_path, event_path!("text"));
    }

    if !log.contains(event_path!("host")) {
        if let Some(host_path) = log.host_path().cloned().as_ref() {
            log.rename_key(host_path, event_path!("host"));
        }
    }

    if !log.contains(event_path!("date_happened")) {
        if let Some(timestamp_path) = log.timestamp_path().cloned().as_ref() {
            log.rename_key(timestamp_path, event_path!("date_happened"));
        }
    }

    if !log.contains(event_path!("source_type_name")) {
        if let Some(source_type_path) = log.source_type_path().cloned().as_ref() {
            log.rename_key(source_type_path, event_path!("source_type_name"));
        }
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
