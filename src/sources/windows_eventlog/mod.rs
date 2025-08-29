use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use tokio::{select, time::interval};
use tokio_stream::wrappers::IntervalStream;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol,
};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    config::LogNamespace,
};
use vrl::value::Kind;

use crate::{
    SourceSender,
    config::{DataType, SourceConfig, SourceContext, SourceOutput},
    internal_events::{EventsReceived, StreamClosedError},
    shutdown::ShutdownSignal,
};

mod config;
pub mod error;
mod parser;
mod subscription;

#[cfg(test)]
mod tests;

pub use self::config::*;
use self::{
    error::WindowsEventLogError, parser::EventLogParser, subscription::EventLogSubscription,
};


/// Windows Event Log source implementation
pub struct WindowsEventLogSource {
    config: WindowsEventLogConfig,
}

impl WindowsEventLogSource {
    pub fn new(config: WindowsEventLogConfig) -> crate::Result<Self> {
        // Validate configuration
        config.validate()?;

        Ok(Self { config })
    }

    async fn run_internal(
        &mut self,
        mut out: SourceSender,
        mut shutdown: ShutdownSignal,
    ) -> Result<(), WindowsEventLogError> {
        let mut subscription = EventLogSubscription::new(&self.config)?;
        let parser = EventLogParser::new(&self.config);

        let events_received = register!(EventsReceived);
        let bytes_received = register!(BytesReceived::from(Protocol::HTTP));

        let poll_interval = Duration::from_secs(self.config.poll_interval_secs);
        let mut interval_stream = IntervalStream::new(interval(poll_interval));

        loop {
            select! {
                _ = &mut shutdown => {
                    info!("Windows Event Log source received shutdown signal");
                    break;
                }

                _ = interval_stream.next() => {
                    debug!(message = "Polling Windows Event Log for events");
                    match subscription.poll_events().await {
                        Ok(events) => {
                            debug!(
                                message = "Polled Windows Event Log",
                                event_count = events.len()
                            );
                            if events.is_empty() {
                                continue;
                            }

                            let mut log_events = Vec::new();
                            let mut total_byte_size = 0;

                            for event in events {
                                match parser.parse_event(event) {
                                    Ok(log_event) => {
                                        let byte_size = log_event.estimated_json_encoded_size_of();
                                        total_byte_size += byte_size.get();
                                        log_events.push(log_event);
                                    }
                                    Err(e) => {
                                        warn!(
                                            message = "Failed to parse Windows event",
                                            error = %e,
                                            internal_log_rate_limit = true
                                        );
                                    }
                                }
                            }

                            if !log_events.is_empty() {
                                let count = log_events.len();
                                events_received.emit(CountByteSize(count, total_byte_size.into()));
                                bytes_received.emit(ByteSize(total_byte_size));

                                if let Err(_error) = out.send_batch(log_events).await {
                                    emit!(StreamClosedError { count });
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                message = "Error polling Windows Event Log",
                                error = %e,
                                internal_log_rate_limit = true
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
#[typetag::serde(name = "windows_eventlog")]
impl SourceConfig for WindowsEventLogConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let source = WindowsEventLogSource::new(self.clone())?;
        Ok(Box::pin(async move {
            let mut source = source;
            if let Err(error) = source.run_internal(cx.out, cx.shutdown).await {
                error!(message = "Windows Event Log source failed", %error);
            }
            Ok(())
        }))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let schema_definition = self
            .log_namespace
            .unwrap_or(false)
            .then(|| {
                vector_lib::schema::Definition::new_with_default_metadata(
                    Kind::object(std::collections::BTreeMap::from([
                        ("timestamp".into(), Kind::timestamp().or_undefined()),
                        ("message".into(), Kind::bytes().or_undefined()),
                        ("level".into(), Kind::bytes().or_undefined()),
                        ("source".into(), Kind::bytes().or_undefined()),
                        ("event_id".into(), Kind::integer().or_undefined()),
                        ("provider_name".into(), Kind::bytes().or_undefined()),
                        ("computer".into(), Kind::bytes().or_undefined()),
                        ("user_id".into(), Kind::bytes().or_undefined()),
                        ("record_id".into(), Kind::integer().or_undefined()),
                        ("activity_id".into(), Kind::bytes().or_undefined()),
                        ("related_activity_id".into(), Kind::bytes().or_undefined()),
                        ("process_id".into(), Kind::integer().or_undefined()),
                        ("thread_id".into(), Kind::integer().or_undefined()),
                        ("channel".into(), Kind::bytes().or_undefined()),
                        ("opcode".into(), Kind::bytes().or_undefined()),
                        ("task".into(), Kind::bytes().or_undefined()),
                        ("keywords".into(), Kind::bytes().or_undefined()),
                    ])),
                    [LogNamespace::Legacy],
                )
            })
            .unwrap_or_else(vector_lib::schema::Definition::any);

        vec![SourceOutput::new_maybe_logs(DataType::Log, schema_definition)]
    }

    fn resources(&self) -> Vec<crate::config::Resource> {
        // Windows Event Logs are local resources
        self.channels
            .iter()
            .map(|channel| crate::config::Resource::DiskBuffer(channel.clone()))
            .collect()
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

use vector_config::component::SourceDescription;

inventory::submit! {
    SourceDescription::new::<WindowsEventLogConfig>("windows_eventlog", "", "", "")
}
