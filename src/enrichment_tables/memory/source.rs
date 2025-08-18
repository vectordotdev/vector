use chrono::Utc;
use futures::StreamExt;
use std::{
    num::NonZeroU64,
    time::{Duration, Instant},
};
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::{
    config::LogNamespace,
    configurable::configurable_component,
    event::{Event, EventMetadata, LogEvent},
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, EventsReceived, InternalEventHandle, Protocol,
    },
    shutdown::ShutdownSignal,
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};

use crate::{internal_events::StreamClosedError, SourceSender};

use super::{Memory, MemoryConfig};

/// Configuration for memory enrichment table source functionality.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemorySourceConfig {
    /// Interval for exporting all data from the table when used as a source.
    pub export_interval: NonZeroU64,
    /// Batch size for data exporting. Used to prevent exporting entire table at
    /// once and blocking the system.
    ///
    /// By default, batches are not used and entire table is exported.
    #[serde(skip_serializing_if = "vector_lib::serde::is_default")]
    pub export_batch_size: Option<u64>,
    /// If set to true, all data will be removed from cache after exporting.
    /// Only valid if used as a source and export_interval > 0
    ///
    /// By default, export will not remove data from cache
    #[serde(default = "crate::serde::default_false")]
    pub remove_after_export: bool,
    /// Key to use for this component when used as a source. This must be different from the
    /// component key.
    pub source_key: String,
}

/// A struct that represents Memory when used as a source.
pub(crate) struct MemorySource {
    pub(super) memory: Memory,
    pub(super) shutdown: ShutdownSignal,
    pub(super) out: SourceSender,
    pub(super) log_namespace: LogNamespace,
}

impl MemorySource {
    pub(crate) async fn run(mut self) -> Result<(), ()> {
        let events_received = register!(EventsReceived);
        let bytes_received = register!(BytesReceived::from(Protocol::INTERNAL));
        let source_config = self
            .memory
            .config
            .source_config
            .as_ref()
            .expect("Unexpected missing source config in memory table used as a source.");
        let mut interval = IntervalStream::new(interval(Duration::from_secs(
            source_config.export_interval.into(),
        )))
        .take_until(self.shutdown);

        while interval.next().await.is_some() {
            let mut sent = 0_usize;
            loop {
                let mut events = Vec::new();
                {
                    let mut writer = self.memory.write_handle.lock().unwrap();
                    if let Some(reader) = self.memory.get_read_handle().read() {
                        let now = Instant::now();
                        let utc_now = Utc::now();
                        events = reader
                            .iter()
                            .skip(if source_config.remove_after_export {
                                0
                            } else {
                                sent
                            })
                            .take(if let Some(batch_size) = source_config.export_batch_size {
                                batch_size as usize
                            } else {
                                usize::MAX
                            })
                            .filter_map(|(k, v)| {
                                if source_config.remove_after_export {
                                    writer.write_handle.empty(k.clone());
                                }
                                v.get_one().map(|v| (k, v))
                            })
                            .filter_map(|(k, v)| {
                                let mut event = Event::Log(LogEvent::from_map(
                                    v.as_object_map(now, self.memory.config.ttl, k).ok()?,
                                    EventMetadata::default(),
                                ));
                                let log = event.as_mut_log();
                                self.log_namespace.insert_standard_vector_source_metadata(
                                    log,
                                    MemoryConfig::NAME,
                                    utc_now,
                                );

                                Some(event)
                            })
                            .collect::<Vec<_>>();
                        if source_config.remove_after_export {
                            writer.write_handle.refresh();
                        }
                    }
                }
                let count = events.len();
                let byte_size = events.size_of();
                let json_size = events.estimated_json_encoded_size_of();
                bytes_received.emit(ByteSize(byte_size));
                events_received.emit(CountByteSize(count, json_size));
                if self.out.send_batch(events).await.is_err() {
                    emit!(StreamClosedError { count });
                }

                sent += count;
                match source_config.export_batch_size {
                    None => break,
                    Some(export_batch_size) if count < export_batch_size as usize => break,
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
