use crate::sinks::util::StreamSink;
use async_graphql::futures_util::stream::BoxStream;
use crate::event::{Event, LogEvent};
use crate::sinks::datadog::events::service::DatadogEventsService;
use futures::StreamExt;
use crate::config::log_schema;
use crate::internal_events::{DatadogEventsProcessed, DatadogEventsFieldInvalid};

pub struct DatadogEventsSink {
    pub service: DatadogEventsService,
}

impl DatadogEventsSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let x = input
            .filter_map(|event| {
                // Panic: This sink only accepts Logs, so this should never panic
                event.into_log()
            })
            .filter_map(ensure_required_fields);

        todo!()
    }
}

fn ensure_required_fields(mut log: LogEvent) -> Option<LogEvent> {
    if !log.contains("title") {
        emit!(&DatadogEventsFieldInvalid { field: "title" });
        return None;
    }

    let log_schema = log_schema();

    if !log.contains("text") {
        if let Some(message) = log.remove(log_schema.message_key()) {
            log.insert("text", message);
        } else {
            emit!(&DatadogEventsFieldInvalid {
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


impl StreamSink for DatadogEventsSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run(input)
    }
}
