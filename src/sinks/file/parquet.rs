//! Batch (columnar) encoding path for the `file` sink.
//!
//! When `batch_encoding` is configured, events are collected into batches and
//! encoded together as complete columnar files (Apache Parquet) rather than
//! written incrementally per event. Because columnar files cannot be appended
//! to, each batch is written to its own file, with a time-ordered UUID (v7)
//! inserted before the extension of the rendered `path` so that successive
//! batches routed to the same path do not overwrite one another.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use tokio::{fs::File, io::AsyncWriteExt};
use uuid::Uuid;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{BatchEncoder, encoding::BatchSerializerConfig},
    internal_event::{CountByteSize, EventsSent, InternalEventHandle as _, Output, Registered},
    json_size::JsonSize,
    partition::Partitioner,
    stream::BatcherSettings,
};

use super::{
    Compression, FileBatchEncoding, FileSinkConfig, OpenError, build_confinement, open_file,
};
use crate::{
    codecs::Transformer,
    config::SinkContext,
    event::{Event, EventStatus, Finalizable},
    internal_events::{
        FileBytesSent, FileIoError, FilePathOutsideBaseDirError, TemplateRenderingError,
    },
    sinks::util::{
        SinkBuilderExt, StreamSink, encoding::Encoder as _, path_confinement::PathConfinement,
        timezone_to_offset,
    },
    template::UnconfinedTemplate,
};

/// A `file` sink that batches events and writes them as complete columnar
/// (Parquet) files.
pub(super) struct ParquetFileSink {
    path: UnconfinedTemplate,
    transformer: Transformer,
    encoder: BatchEncoder,
    batcher_settings: BatcherSettings,
    events_sent: Registered<EventsSent>,
    include_file_metric_tag: bool,
    confinement: Option<PathConfinement>,
}

/// Partitions events by their rendered `path`. Events whose template fails to
/// render are grouped under the `None` key and skipped by the sink.
///
/// Confinement is applied once per batch in
/// [`ParquetFileSink::write_batch`] rather than here: the rendered path *is*
/// the partition key, so every event in a batch shares one path, and the check
/// still runs before any filesystem mutation.
struct PathPartitioner {
    path: UnconfinedTemplate,
}

impl Partitioner for PathPartitioner {
    type Item = Event;
    type Key = Option<String>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        self.path
            .render_string(item)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("path"),
                    drop_event: true,
                });
            })
            .ok()
    }
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
            confinement: build_confinement(config)?,
        })
    }

    async fn run_inner(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let partitioner = PathPartitioner {
            path: self.path.clone(),
        };
        let settings = self.batcher_settings;

        let mut batches = input.batched_partitioned(partitioner, settings.timeout, move |_| {
            settings.as_byte_size_config()
        });

        while let Some((key, mut batch)) = batches.next().await {
            match key {
                Some(path) => self.write_batch(path, batch).await,
                // Events whose path template failed to render are grouped under
                // a `None` key. The rendering error (and per-event drop) is
                // already reported by the partitioner, but the batch is still
                // discarded here without being written, so we must mark its
                // finalizers as errored. Otherwise the acknowledgement batch
                // stays at its default `Delivered` status and sources using
                // end-to-end acknowledgements would be told these unwritten
                // events succeeded.
                None => batch.take_finalizers().update_status(EventStatus::Errored),
            }
        }

        Ok(())
    }

    async fn write_batch(&mut self, path: String, mut events: Vec<Event>) {
        let finalizers = events.take_finalizers();
        let event_count = events.len();
        let events_size: JsonSize = events
            .iter()
            .map(EstimatedJsonEncodedSizeOf::estimated_json_encoded_size_of)
            .sum();

        // Each batch lands in its own file, so it is the per-batch unique path
        // — not the partition key — that must be confined. Runs before any
        // filesystem mutation.
        let file_path = unique_batch_path(&path);
        let file_path = match self.confinement.as_ref() {
            Some(confinement) => match confinement.confine(&file_path) {
                Ok(normalized) => normalized,
                Err(error) => {
                    finalizers.update_status(EventStatus::Errored);
                    emit!(FilePathOutsideBaseDirError {
                        path: &file_path,
                        base_dir: confinement.base_dir(),
                        error,
                        dropped_events: event_count,
                    });
                    return;
                }
            },
            None => file_path,
        };

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

        // Capture the base before `confinement` is borrowed mutably below.
        let base_dir = self
            .confinement
            .as_ref()
            .map(|confinement| confinement.base_dir().to_path_buf())
            .unwrap_or_default();

        // Open through the shared helper so the batch path gets the same
        // no-follow directory creation, parent verification, and `O_NOFOLLOW`
        // treatment as the streaming path.
        let mut file = match open_file(&file_path, true, self.confinement.as_mut()).await {
            Ok(file) => file,
            Err(OpenError::Confine(error)) => {
                finalizers.update_status(EventStatus::Errored);
                emit!(FilePathOutsideBaseDirError {
                    path: &file_path,
                    base_dir: &base_dir,
                    error,
                    dropped_events: event_count,
                });
                return;
            }
            Err(OpenError::Io(error)) => {
                finalizers.update_status(EventStatus::Errored);
                emit!(FileIoError {
                    code: "failed_opening_file",
                    message: "Unable to open the file.",
                    error,
                    path: &file_path,
                    dropped_events: event_count,
                });
                return;
            }
        };

        match write_file(&mut file, &buffer).await {
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
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Build a file path for a batch by inserting a time-ordered UUID (v7) into the
/// rendered `path` before its extension (for example `events.parquet` becomes
/// `events-0190f8c1-9a7b-7c3d-8e2f-1a2b3c4d5e6f.parquet`). This keeps successive
/// batches routed to the same rendered path from overwriting one another, since
/// columnar files cannot be appended to. A UUID rather than a plain millisecond
/// timestamp guarantees a distinct filename even when multiple batches are
/// flushed within the same millisecond (or the clock repeats a value).
fn unique_batch_path(rendered: &str) -> PathBuf {
    let path = Path::new(rendered);
    let id = Uuid::now_v7();

    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    let extension = path.extension().map(|s| s.to_string_lossy().into_owned());

    let file_name = match (stem, extension) {
        (Some(stem), Some(ext)) => format!("{stem}-{id}.{ext}"),
        (Some(stem), None) => format!("{stem}-{id}"),
        (None, Some(ext)) => format!("{id}.{ext}"),
        (None, None) => id.to_string(),
    };

    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

async fn write_file(file: &mut File, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(bytes).await?;
    file.sync_all().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_batch_path_inserts_uuid_before_extension() {
        let path = unique_batch_path("/tmp/vector-2026-07-08.parquet");
        let file_name = path.file_name().unwrap().to_string_lossy();

        assert_eq!(path.parent().unwrap().to_str().unwrap(), "/tmp");
        assert!(file_name.starts_with("vector-2026-07-08-"));
        assert!(file_name.ends_with(".parquet"));

        // The inserted segment is a valid, time-ordered (v7) UUID.
        let id = file_name
            .strip_prefix("vector-2026-07-08-")
            .and_then(|name| name.strip_suffix(".parquet"))
            .expect("uuid segment should sit between the stem and extension");
        let parsed = Uuid::parse_str(id).expect("inserted segment should be a valid UUID");
        assert_eq!(parsed.get_version(), Some(uuid::Version::SortRand));
    }

    #[test]
    fn unique_batch_path_without_extension() {
        let path = unique_batch_path("/tmp/vector");
        let file_name = path.file_name().unwrap().to_string_lossy();

        assert!(file_name.starts_with("vector-"));
        assert!(!file_name.contains('.'));
    }

    #[test]
    fn unique_batch_path_avoids_collisions_within_a_millisecond() {
        // Generate many paths in a tight loop (well within a single
        // millisecond) and confirm every filename is distinct — the property a
        // plain millisecond timestamp could not guarantee.
        let unique: std::collections::HashSet<_> = (0..1000)
            .map(|_| unique_batch_path("/tmp/events.parquet"))
            .collect();
        assert_eq!(unique.len(), 1000, "each batch path should be unique");
    }
}
