//! Batch (columnar) encoding path for the `file` sink.
//!
//! When `batch_encoding` is configured, events are collected into batches and
//! encoded together as complete columnar files (Apache Parquet) rather than
//! written incrementally per event. Because columnar files cannot be appended
//! to, each batch is written to its own file, with a millisecond timestamp
//! inserted before the extension of the rendered `path` so that successive
//! batches routed to the same path do not overwrite one another.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::{BoxStream, StreamExt};
use tokio::{fs, io::AsyncWriteExt};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{BatchEncoder, encoding::BatchSerializerConfig},
    internal_event::{CountByteSize, EventsSent, InternalEventHandle as _, Output, Registered},
    json_size::JsonSize,
    stream::BatcherSettings,
};

use super::{Compression, FileBatchEncoding, FileSinkConfig};
use crate::{
    codecs::Transformer,
    config::SinkContext,
    event::{Event, EventStatus, Finalizable},
    internal_events::{FileBytesSent, FileIoError},
    sinks::util::{
        SinkBuilderExt, StreamSink, encoding::Encoder as _, partitioner::KeyPartitioner,
        timezone_to_offset,
    },
    template::Template,
};

/// A `file` sink that batches events and writes them as complete columnar
/// (Parquet) files.
pub(super) struct ParquetFileSink {
    path: Template,
    transformer: Transformer,
    encoder: BatchEncoder,
    batcher_settings: BatcherSettings,
    events_sent: Registered<EventsSent>,
    include_file_metric_tag: bool,
}

impl ParquetFileSink {
    pub(super) fn new(config: &FileSinkConfig, cx: SinkContext) -> crate::Result<Self> {
        let FileBatchEncoding::Parquet(parquet_config) = config
            .batch_encoding
            .as_ref()
            .expect("batch_encoding must be set to build a ParquetFileSink");

        let batch_serializer =
            BatchSerializerConfig::Parquet(parquet_config.clone()).build_batch_serializer()?;
        let encoder = BatchEncoder::new(batch_serializer);

        if config.compression != Compression::None {
            warn!(
                message = "The top-level `compression` setting is ignored when `batch_encoding` is set to parquet; Parquet handles compression internally."
            );
        }

        let offset = config
            .timezone
            .or(cx.globals.timezone)
            .and_then(timezone_to_offset);

        Ok(Self {
            path: config.path.clone().with_tz_offset(offset),
            transformer: config.encoding.transformer(),
            encoder,
            batcher_settings: config.batch.into_batcher_settings()?,
            events_sent: register!(EventsSent::from(Output(None))),
            include_file_metric_tag: config.internal_metrics.include_file_tag,
        })
    }

    async fn run_inner(&self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let partitioner = KeyPartitioner::new(self.path.clone(), None);
        let settings = self.batcher_settings;

        let mut batches = input.batched_partitioned(partitioner, settings.timeout, move |_| {
            settings.as_byte_size_config()
        });

        while let Some((key, batch)) = batches.next().await {
            // Events whose path template failed to render are grouped under a
            // `None` key; the rendering error (and drop) is already reported by
            // the partitioner, so we simply skip them here.
            if let Some(path) = key {
                self.write_batch(path, batch).await;
            }
        }

        Ok(())
    }

    async fn write_batch(&self, path: String, mut events: Vec<Event>) {
        let finalizers = events.take_finalizers();
        let event_count = events.len();
        let events_size: JsonSize = events
            .iter()
            .map(EstimatedJsonEncodedSizeOf::estimated_json_encoded_size_of)
            .sum();

        let mut buffer = Vec::new();
        if let Err(error) =
            (self.transformer.clone(), self.encoder.clone()).encode_input(events, &mut buffer)
        {
            // The codec emits its own error/drop internal events, so here we only
            // mark the finalizers as errored to signal the failure upstream.
            finalizers.update_status(EventStatus::Errored);
            emit!(FileIoError {
                code: "failed_encoding_batch",
                message: "Failed to batch encode events.",
                error,
                path: &path,
                dropped_events: 0,
            });
            return;
        }

        let file_path = timestamped_batch_path(&path);
        match write_file(&file_path, &buffer).await {
            Ok(()) => {
                finalizers.update_status(EventStatus::Delivered);
                self.events_sent
                    .emit(CountByteSize(event_count, events_size));
                emit!(FileBytesSent {
                    byte_size: buffer.len(),
                    file: file_path.to_string_lossy(),
                    include_file_metric_tag: self.include_file_metric_tag,
                });
            }
            Err(error) => {
                finalizers.update_status(EventStatus::Errored);
                emit!(FileIoError {
                    code: "failed_writing_file",
                    message: "Failed to write the file.",
                    error,
                    path: &file_path,
                    dropped_events: event_count,
                });
            }
        }
    }
}

#[async_trait]
impl StreamSink<Event> for ParquetFileSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Build a file path for a batch by inserting the current time, as a Unix
/// millisecond timestamp, into the rendered `path` before its extension (for
/// example `events.parquet` becomes `events-1751932800123.parquet`). This keeps
/// successive batches routed to the same rendered path from overwriting one
/// another, since columnar files cannot be appended to.
fn timestamped_batch_path(rendered: &str) -> PathBuf {
    let path = Path::new(rendered);
    let timestamp = Utc::now().timestamp_millis();

    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    let extension = path.extension().map(|s| s.to_string_lossy().into_owned());

    let file_name = match (stem, extension) {
        (Some(stem), Some(ext)) => format!("{stem}-{timestamp}.{ext}"),
        (Some(stem), None) => format!("{stem}-{timestamp}"),
        (None, Some(ext)) => format!("{timestamp}.{ext}"),
        (None, None) => timestamp.to_string(),
    };

    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

async fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).await?;
    }

    let mut file = fs::File::create(path).await?;
    file.write_all(bytes).await?;
    file.sync_all().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamped_batch_path_inserts_timestamp_before_extension() {
        let path = timestamped_batch_path("/tmp/vector-2026-07-08.parquet");
        let file_name = path.file_name().unwrap().to_string_lossy();

        assert_eq!(path.parent().unwrap().to_str().unwrap(), "/tmp");
        assert!(file_name.starts_with("vector-2026-07-08-"));
        assert!(file_name.ends_with(".parquet"));

        // The inserted segment is a positive numeric millisecond timestamp.
        let timestamp = file_name
            .strip_prefix("vector-2026-07-08-")
            .and_then(|name| name.strip_suffix(".parquet"))
            .expect("timestamp segment should sit between the stem and extension");
        assert!(timestamp.parse::<i64>().unwrap() > 0);
    }

    #[test]
    fn timestamped_batch_path_without_extension() {
        let path = timestamped_batch_path("/tmp/vector");
        let file_name = path.file_name().unwrap().to_string_lossy();

        assert!(file_name.starts_with("vector-"));
        assert!(!file_name.contains('.'));
    }
}
