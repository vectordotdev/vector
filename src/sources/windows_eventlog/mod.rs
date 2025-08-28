use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use snafu::{ResultExt, Snafu};
use tokio::{select, sync::mpsc, time::interval};
use tokio_stream::wrappers::IntervalStream;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol, Registered};
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    EstimatedJsonEncodedSizeOf,
};
use vrl::value::{Kind, Value};

use crate::{
    config::{SourceConfig, SourceContext, SourceOutput, DataType, log_schema},
    event::LogEvent,
    internal_events::{EventsReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    SourceSender,
};

mod config;
mod error;
mod parser;
mod subscription;

#[cfg(test)]
mod tests;

pub use self::config::*;
use self::{
    error::WindowsEventLogError,
    parser::EventLogParser,
    subscription::EventLogSubscription,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid configuration: {}", message))]
    InvalidConfiguration { message: String },
    #[snafu(display("Windows Event Log API error: {}", source))]
    WindowsEventLogApiError { source: WindowsEventLogError },
}

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
        shutdown: ShutdownSignal,
    ) -> Result<(), WindowsEventLogError> {
        let mut subscription = EventLogSubscription::new(&self.config)?;
        let mut parser = EventLogParser::new(&self.config);
        
        let mut events_received = register!(EventsReceived);
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
                    match subscription.poll_events().await {
                        Ok(events) => {
                            if events.is_empty() {
                                continue;
                            }

                            let mut log_events = Vec::new();
                            let mut total_byte_size = 0;

                            for event in events {
                                match parser.parse_event(event) {
                                    Ok(log_event) => {
                                        let byte_size = log_event.estimated_json_encoded_size_of();
                                        total_byte_size += byte_size;
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
                                events_received.emit(count);
                                bytes_received.emit(ByteSize(total_byte_size));

                                if let Err(error) = out.send_batch(log_events).await {
                                    emit!(StreamClosedError { error, count });
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
impl SourceConfig for WindowsEventLogConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let source = WindowsEventLogSource::new(self.clone())?;
        Ok(Box::pin(async move {
            let mut source = source;
            if let Err(error) = source.run_internal(cx.out, cx.shutdown).await {
                error!(message = "Windows Event Log source failed", %error);
            }
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
                    &log_schema().log_namespace(),
                )
            })
            .unwrap_or_else(vector_lib::schema::Definition::any);

        vec![SourceOutput::new_logs(DataType::Log, schema_definition)]
    }

    fn resources(&self) -> Vec<crate::config::Resource> {
        // Windows Event Logs are local resources
        self.channels
            .iter()
            .map(|channel| crate::config::Resource::from(format!("windows_eventlog:{}", channel)))
            .collect()
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

inventory::submit! {
    crate::config::SourceDescription::new::<WindowsEventLogConfig>("windows_eventlog")
}