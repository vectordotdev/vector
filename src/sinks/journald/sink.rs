use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use snafu::Snafu;
use tracing::{error, warn};
use vector_lib::EstimatedJsonEncodedSizeOf;
use vector_lib::internal_event::{
    ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle as _, Output, Protocol,
};
use vrl::core::Value;

use crate::sinks::journald::journald_writer::JournaldWriter;
use crate::{
    event::{Event, EventStatus, Finalizable, LogEvent},
    sinks::{journald::config::JournaldSinkConfig, util::StreamSink},
};

#[derive(Debug, Snafu)]
pub enum JournaldSinkError {
    #[snafu(display("Failed to send log to journald: {}", source))]
    Send { source: std::io::Error },
    #[snafu(display("Failed to initialize journald writer: {}", source))]
    Init { source: std::io::Error },
}

pub struct JournaldSink {
    writer: JournaldWriter,
}

impl JournaldSink {
    pub fn new(config: JournaldSinkConfig) -> crate::Result<Self> {
        let writer = JournaldWriter::new(&config.journald_path)
            .map_err(|e| JournaldSinkError::Init { source: e })?;
        Ok(Self { writer })
    }

    /// Sends a log event to the journald writer.
    /// This method extracts all fields from the log event and sends them to journald.
    /// It handles different value types appropriately, converting them to strings or bytes as needed.
    /// Returns the number of bytes sent to journald.
    pub async fn send_log_to_journal(
        &mut self,
        log: &LogEvent,
    ) -> Result<usize, JournaldSinkError> {
        // Add other relevant fields from the log event
        if let Some(all_fields) = log.all_event_fields() {
            for (key, value) in all_fields {
                let key_str = key.to_string();

                let k = key_str.as_str();
                match value {
                    Value::Bytes(v) => {
                        self.writer.add_bytes(k, v);
                    }
                    Value::Regex(v) => {
                        self.writer.add_str(k, &v.to_string());
                    }
                    Value::Integer(_) | Value::Float(_) => {
                        self.writer.add_str(k, &value.to_string());
                    }
                    Value::Boolean(v) => {
                        self.writer.add_str(k, if *v { "true" } else { "false" });
                    }
                    Value::Timestamp(v) => {
                        self.writer.add_str(k, &v.to_rfc3339());
                    }
                    Value::Object(_) | Value::Array(_) => {
                        // Currently this code is unreachable because `all_event_fields` flattens
                        // the event fields and does not include complex types like Object or Array.
                        warn!(
                            "Journald sink does not support sending complex types like Object or Array. Key: {k}"
                        );
                        continue;
                    }
                    Value::Null => {
                        self.writer.add_str(k, "null");
                        continue;
                    }
                }
            }
        }

        let bytes_sent = self
            .writer
            .write()
            .await
            .map_err(|err| JournaldSinkError::Send { source: err })?;

        Ok(bytes_sent)
    }
}

#[async_trait]
impl StreamSink<Event> for JournaldSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let events_sent = register!(EventsSent::from(Output(None)));
        let bytes_sent = register!(BytesSent::from(Protocol("journald".into())));

        while let Some(mut event) = input.next().await {
            let event_byte_size = event.estimated_json_encoded_size_of();
            let finalizers = event.take_finalizers();

            match event {
                Event::Log(ref log) => match self.send_log_to_journal(log).await {
                    Ok(bytes_written) => {
                        finalizers.update_status(EventStatus::Delivered);
                        events_sent.emit(CountByteSize(1, event_byte_size));
                        bytes_sent.emit(ByteSize(bytes_written));
                    }
                    Err(error) => {
                        error!(message = "Failed to send event to journald.", %error);
                        finalizers.update_status(EventStatus::Errored);
                    }
                },
                _ => {
                    // For non-log events, we don't process them
                    finalizers.update_status(EventStatus::Errored);
                    warn!("Journald sink received non-log event, skipping.");
                }
            }
        }

        Ok(())
    }
}
