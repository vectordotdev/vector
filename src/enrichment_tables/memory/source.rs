use std::time::{Duration, Instant};

use chrono::Utc;
use futures::StreamExt;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    config::LogNamespace,
    event::{Event, EventMetadata, LogEvent},
    internal_event::{
        ByteSize, BytesReceived, BytesReceivedHandle, CountByteSize, EventsReceived,
        EventsReceivedHandle, InternalEventHandle, Protocol,
    },
    shutdown::ShutdownSignal,
};

use super::{Memory, MemoryConfig};
use crate::{
    SourceSender,
    enrichment_tables::memory::{MemoryEntryPair, MemorySourceConfig},
    internal_events::StreamClosedError,
};

pub(crate) const EXPIRED_ROUTE: &str = "expired";

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
            .clone()
            .expect("Unexpected missing source config in memory table used as a source.");
        let mut interval = IntervalStream::new(interval(Duration::from_secs(
            source_config
                .export_interval
                .map(Into::into)
                .unwrap_or(u64::MAX),
        )))
        .take_until(self.shutdown.clone());
        let mut expired_receiver = self.memory.subscribe_to_expired_items();

        loop {
            tokio::select! {
                interval_time = interval.next() => {
                    if interval_time.is_none() {
                        break;
                    }
                    self.export_table_items(&source_config, &events_received, &bytes_received).await;
                },

                Ok(expired) = expired_receiver.recv() => {
                    self.export_expired_entries(expired, &events_received, &bytes_received).await;
                }
            }
        }

        Ok(())
    }

    async fn export_table_items(
        &mut self,
        source_config: &MemorySourceConfig,
        events_received: &EventsReceivedHandle,
        bytes_received: &BytesReceivedHandle,
    ) {
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
                                v.as_object_map(now, k).ok()?,
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

    async fn export_expired_entries(
        &mut self,
        entries: Vec<MemoryEntryPair>,
        events_received: &EventsReceivedHandle,
        bytes_received: &BytesReceivedHandle,
    ) {
        let now = Instant::now();
        let events = entries
            .into_iter()
            .filter_map(
                |MemoryEntryPair {
                     key,
                     entry: expired_event,
                 }| {
                    let mut event = Event::Log(LogEvent::from_map(
                        expired_event.as_object_map(now, &key).ok()?,
                        EventMetadata::default(),
                    ));
                    let log = event.as_mut_log();
                    self.log_namespace.insert_standard_vector_source_metadata(
                        log,
                        MemoryConfig::NAME,
                        Utc::now(),
                    );
                    Some(event)
                },
            )
            .collect::<Vec<_>>();
        let count = events.len();
        let byte_size = events.size_of();
        let json_size = events.estimated_json_encoded_size_of();
        bytes_received.emit(ByteSize(byte_size));
        events_received.emit(CountByteSize(count, json_size));
        if self
            .out
            .send_batch_named(EXPIRED_ROUTE, events)
            .await
            .is_err()
        {
            emit!(StreamClosedError { count });
        }
    }
}
