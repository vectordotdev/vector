use std::{
    convert::TryFrom,
    num::NonZeroU64,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use async_compression::tokio::write::{GzipEncoder, ZstdEncoder};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures::{
    FutureExt, future,
    stream::{BoxStream, StreamExt},
};
use serde_with::serde_as;
use tokio::{
    fs::{self, File},
    io::{AsyncSeekExt, AsyncWrite, AsyncWriteExt},
};
use tokio_util::{codec::Encoder as _, time::delay_queue::Expired};
use vector_lib::{
    EstimatedJsonEncodedSizeOf, TimeZone,
    codecs::{
        TextSerializerConfig,
        encoding::{Framer, FramingConfig},
    },
    configurable::configurable_component,
    finalization::EventFinalizers,
    internal_event::{CountByteSize, EventsSent, InternalEventHandle as _, Output, Registered},
    json_size::JsonSize,
    partition::Partitioner,
    stream::BatcherSettings,
};

use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType, Transformer},
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    event::{Event, EventStatus, Finalizable},
    expiring_hash_map::ExpiringHashMap,
    internal_events::{
        FileBytesSent, FileInternalMetricsConfig, FileIoError, FileOpen,
        FilePathOutsideBaseDirError, TemplateRenderingError,
    },
    sinks::util::{
        BatchConfig, RealtimeSizeBasedDefaultBatchSettings, StreamSink,
        path_confinement::{ConfineError, PathConfinement},
        timezone_to_offset,
    },
    template::{ConfinementConfig, UnconfinedTemplate},
};

mod bytes_path;

use bytes_path::BytesPath;

/// Configuration for the `file` sink.
#[serde_as]
#[configurable_component(sink("file", "Output observability events into files."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileSinkConfig {
    /// File path to write events to.
    ///
    /// Compression format extension must be explicit.
    #[configurable(metadata(docs::examples = "/var/log/vector/vector-%Y-%m-%d.log"))]
    #[configurable(metadata(
        docs::examples = "/tmp/application-{{ application_id }}-%Y-%m-%d.log"
    ))]
    #[configurable(metadata(docs::examples = "/tmp/vector-%Y-%m-%d.log.zst"))]
    #[configurable(metadata(
        docs::warnings = "Rendered paths are confined to `base_dir` (derived from the literal prefix of `path` when unset). See the `base_dir` option."
    ))]
    pub path: UnconfinedTemplate,

    /// Directory under which all rendered `path` values must resolve.
    ///
    /// When `path` contains event-field references (`{{ field }}`), Vector
    /// confines every rendered path to this directory. If unset, the base
    /// directory is derived from the literal prefix of `path` (the portion
    /// before the first `{{` or `%`). Configuration fails if `path`
    /// references event fields and no non-root base directory can be
    /// derived.
    #[configurable(metadata(docs::examples = "/var/log/vector"))]
    #[serde(default)]
    pub base_dir: Option<PathBuf>,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,

    /// The amount of time that a file can be idle and stay open.
    ///
    /// After not receiving any events in this amount of time, the file is flushed and closed.
    #[serde(default = "default_idle_timeout")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(rename = "idle_timeout_secs")]
    #[configurable(metadata(docs::examples = 600))]
    #[configurable(metadata(docs::human_name = "Idle Timeout"))]
    pub idle_timeout: Duration,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub compression: Compression,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(default)]
    pub timezone: Option<TimeZone>,

    #[serde(default)]
    pub internal_metrics: FileInternalMetricsConfig,

    #[serde(default)]
    pub truncate: FileTruncateConfig,

    /// Controls how events are batched per destination file before writing.
    ///
    /// Events sharing the same rendered path are accumulated into a single buffer and written
    /// with one syscall per batch, reducing overhead when routing to many partitions
    /// (for example, one file per Kafka topic). The default timeout is 1 second; raising it
    /// increases throughput at the cost of end-to-end latency.
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
}

/// Configuration for truncating files.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct FileTruncateConfig {
    /// If this is set, files will be truncated after being closed for a set amount of seconds.
    #[serde(default)]
    pub after_close_time_secs: Option<NonZeroU64>,
    /// If this is set, files will be truncated after set amount of seconds of no modifications.
    #[serde(default)]
    pub after_modified_time_secs: Option<NonZeroU64>,
    /// If this is set, files will be truncated after set amount of seconds regardless of the state.
    #[serde(default)]
    pub after_secs: Option<NonZeroU64>,
}

impl GenerateConfig for FileSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            path: UnconfinedTemplate::try_from("/tmp/vector-%Y-%m-%d.log").unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Default::default(),
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: Default::default(),
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: Default::default(),
        })
        .unwrap()
    }
}

const fn default_idle_timeout() -> Duration {
    Duration::from_secs(30)
}

/// Compression configuration.
// TODO: Why doesn't this already use `crate::sinks::util::Compression`
// `crate::sinks::util::Compression` doesn't support zstd yet
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    /// [Gzip][gzip] compression.
    ///
    /// [gzip]: https://www.gzip.org/
    Gzip,

    /// [Zstandard][zstd] compression.
    ///
    /// [zstd]: https://facebook.github.io/zstd/
    Zstd,

    /// No compression.
    #[default]
    None,
}

/// Wraps a file, counting bytes written beneath a compression encoder.
struct CountingFile {
    file: File,
    written_since_mark: u64,
}

impl CountingFile {
    const fn new(file: File) -> Self {
        Self {
            file,
            written_since_mark: 0,
        }
    }

    const fn mark(&mut self) {
        self.written_since_mark = 0;
    }

    const fn written_since_mark(&self) -> u64 {
        self.written_since_mark
    }

    fn into_file(self) -> File {
        self.file
    }

    async fn sync_all(&self) -> std::io::Result<()> {
        self.file.sync_all().await
    }

    async fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        self.file.metadata().await
    }
}

impl AsyncWrite for CountingFile {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.file).poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                this.written_since_mark += n as u64;
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().file).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().file).poll_shutdown(cx)
    }
}

struct OutFile {
    created_at: Instant,
    compression: Compression,
    inner: OutFileInner,
}

enum OutFileInner {
    Regular(File),
    Gzip(GzipEncoder<CountingFile>),
    Zstd(ZstdEncoder<CountingFile>),
    /// Transient placeholder used only inside `OutFile::reset` while the
    /// inner file is being rewound and a fresh compression stream built.
    Empty,
}

impl OutFile {
    fn new(file: File, compression: Compression) -> Self {
        Self {
            created_at: Instant::now(),
            compression,
            inner: match compression {
                Compression::None => OutFileInner::Regular(file),
                Compression::Gzip => OutFileInner::Gzip(GzipEncoder::new(CountingFile::new(file))),
                Compression::Zstd => OutFileInner::Zstd(ZstdEncoder::new(CountingFile::new(file))),
            },
        }
    }

    async fn sync_all(&mut self) -> Result<(), std::io::Error> {
        match &mut self.inner {
            OutFileInner::Regular(file) => file.sync_all().await,
            OutFileInner::Gzip(gzip) => gzip.get_mut().sync_all().await,
            OutFileInner::Zstd(zstd) => zstd.get_mut().sync_all().await,
            OutFileInner::Empty => unreachable!("OutFileInner::Empty is transient"),
        }
    }

    async fn shutdown(&mut self) -> Result<(), std::io::Error> {
        match &mut self.inner {
            OutFileInner::Regular(file) => file.shutdown().await,
            OutFileInner::Gzip(gzip) => gzip.shutdown().await,
            OutFileInner::Zstd(zstd) => zstd.shutdown().await,
            OutFileInner::Empty => unreachable!("OutFileInner::Empty is transient"),
        }
    }

    async fn write(&mut self, src: &[u8]) -> Result<usize, std::io::Error> {
        match &mut self.inner {
            OutFileInner::Regular(file) => file.write(src).await,
            OutFileInner::Gzip(gzip) => gzip.write(src).await,
            OutFileInner::Zstd(zstd) => zstd.write(src).await,
            OutFileInner::Empty => unreachable!("OutFileInner::Empty is transient"),
        }
    }

    async fn len(&mut self) -> Result<u64, std::io::Error> {
        match &mut self.inner {
            OutFileInner::Regular(file) => file.metadata().await.map(|m| m.len()),
            OutFileInner::Gzip(gzip) => gzip.get_mut().metadata().await.map(|m| m.len()),
            OutFileInner::Zstd(zstd) => zstd.get_mut().metadata().await.map(|m| m.len()),
            OutFileInner::Empty => unreachable!("OutFileInner::Empty is transient"),
        }
    }

    fn mark_written(&mut self) {
        match &mut self.inner {
            OutFileInner::Regular(_) => {}
            OutFileInner::Gzip(gzip) => gzip.get_mut().mark(),
            OutFileInner::Zstd(zstd) => zstd.get_mut().mark(),
            OutFileInner::Empty => unreachable!("OutFileInner::Empty is transient"),
        }
    }

    fn written_bytes(&self) -> u64 {
        match &self.inner {
            OutFileInner::Regular(_) => 0,
            OutFileInner::Gzip(gzip) => gzip.get_ref().written_since_mark(),
            OutFileInner::Zstd(zstd) => zstd.get_ref().written_since_mark(),
            OutFileInner::Empty => unreachable!("OutFileInner::Empty is transient"),
        }
    }

    /// Rewinds the file to `size` and installs a fresh compression stream.
    async fn reset(&mut self, size: u64) -> Result<(), std::io::Error> {
        if let Err(error) = self.shutdown().await {
            warn!(
                message = "Failed to complete compression stream while resetting the file.",
                error = ?error,
            );
        }
        let mut file = match std::mem::replace(&mut self.inner, OutFileInner::Empty) {
            OutFileInner::Regular(file) => file,
            OutFileInner::Gzip(gzip) => gzip.into_inner().into_file(),
            OutFileInner::Zstd(zstd) => zstd.into_inner().into_file(),
            OutFileInner::Empty => unreachable!("OutFileInner::Empty is transient"),
        };
        let result = async {
            file.set_len(size).await?;
            file.seek(std::io::SeekFrom::Start(size)).await?;
            Ok(())
        }
        .await;
        self.inner = match self.compression {
            Compression::None => OutFileInner::Regular(file),
            Compression::Gzip => OutFileInner::Gzip(GzipEncoder::new(CountingFile::new(file))),
            Compression::Zstd => OutFileInner::Zstd(ZstdEncoder::new(CountingFile::new(file))),
        };
        result
    }

    /// Completes the current compression stream and starts a fresh one at the
    /// end of the file.
    async fn finish_and_reopen(&mut self) -> Result<(), std::io::Error> {
        self.shutdown().await?;
        let end = self.len().await?;
        self.reset(end).await
    }

    const fn created_at(&self) -> Instant {
        self.created_at
    }

    /// Shutdowns by flushing data, writing headers, and syncing all of that
    /// data and metadata to the filesystem.
    async fn close(&mut self) -> Result<(), std::io::Error> {
        self.shutdown().await?;
        self.sync_all().await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "file")]
impl SinkConfig for FileSinkConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedFileSink {
    transformer: Transformer,
    batch_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for FileSinkConfig {
    type Validated = ValidatedFileSink;

    fn validate(&self) -> crate::Result<ValidatedFileSink> {
        self.encoding.validate()?;
        let transformer = self.encoding.transformer();

        // Pure path-confinement checks. `PathConfinement` itself is not
        // retained (it is not `Clone`), so `build` reconstructs it from the
        // same inputs; running the checks here lets `vector validate
        // --no-environment` catch invalid routing paths and relative
        // `base_dir`s.
        if let Some(base) = self.base_dir.as_ref()
            && base.is_relative()
        {
            return Err(Box::new(
                crate::sinks::util::path_confinement::BuildError::BaseNotAbsolute {
                    path: base.clone(),
                },
            ));
        }
        if !self
            .confinement
            .dangerously_allow_unconfined_template_resolution
        {
            PathConfinement::for_template(&self.path, self.base_dir.as_deref())
                .map_err(Box::new)?;
        }

        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        Ok(ValidatedFileSink {
            transformer,
            batch_settings,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedFileSink,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = FileSink::from_validated(self, validated, cx)?;
        Ok((
            super::VectorSink::from_event_streamsink(sink),
            future::ok(()).boxed(),
        ))
    }
}

pub struct FileSink {
    path: UnconfinedTemplate,
    transformer: Transformer,
    encoder: Encoder<Framer>,
    idle_timeout: Duration,
    batch_settings: BatcherSettings,
    files: ExpiringHashMap<Bytes, OutFile>,
    compression: Compression,
    events_sent: Registered<EventsSent>,
    include_file_metric_tag: bool,
    truncation_config: FileTruncateConfig,
    confinement: Option<PathConfinement>,
}

impl FileSink {
    pub fn new(config: &FileSinkConfig, cx: SinkContext) -> crate::Result<Self> {
        let validated = config.validate()?;
        Self::from_validated(config, &validated, cx)
    }

    /// Constructs the sink from the validated state, performing only the
    /// environment-dependent (non-`validate`) work: timezone offset resolution
    /// and path confinement construction.
    fn from_validated(
        config: &FileSinkConfig,
        validated: &ValidatedFileSink,
        cx: SinkContext,
    ) -> crate::Result<Self> {
        let offset = config
            .timezone
            .or(cx.globals.timezone)
            .and_then(timezone_to_offset);

        // Config validation runs regardless of the opt-out: a relative
        // `base_dir` is a syntactic error, not a confinement decision.
        if let Some(base) = config.base_dir.as_ref()
            && base.is_relative()
        {
            return Err(Box::new(
                crate::sinks::util::path_confinement::BuildError::BaseNotAbsolute {
                    path: base.clone(),
                },
            ));
        }

        let confinement = if config
            .confinement
            .dangerously_allow_unconfined_template_resolution
        {
            ConfinementConfig::warn_unconfined_template("sink", "file", "path");
            None
        } else {
            PathConfinement::for_template(&config.path, config.base_dir.as_deref())
                .map_err(Box::new)?
        };

        let (framer, serializer) = config.encoding.build(SinkType::StreamBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        Ok(Self {
            path: config.path.clone().with_tz_offset(offset),
            transformer: validated.transformer.clone(),
            encoder,
            idle_timeout: config.idle_timeout,
            batch_settings: validated.batch_settings,
            files: ExpiringHashMap::default(),
            compression: config.compression,
            events_sent: register!(EventsSent::from(Output(None))),
            include_file_metric_tag: config.internal_metrics.include_file_tag,
            truncation_config: config.truncate.clone(),
            confinement,
        })
    }

    fn deadline_at(&self) -> Instant {
        Instant::now()
            .checked_add(self.idle_timeout)
            .expect("unable to compute next deadline")
    }

    async fn run(&mut self, input: BoxStream<'_, Event>) -> crate::Result<()> {
        let partitioner = FilePathPartitioner {
            path: self.path.clone(),
        };
        let batch_settings = self.batch_settings;
        // Per-path buffers of `(events, generation, bytes)`; `bytes` tracks
        // size without rescanning, `generation` marks stale `flush_deadlines`.
        let mut buffers: std::collections::HashMap<Bytes, (Vec<Event>, u64, usize)> =
            std::collections::HashMap::new();
        let mut per_path_gen: std::collections::HashMap<Bytes, u64> =
            std::collections::HashMap::new();
        let mut flush_deadlines: std::collections::BinaryHeap<
            std::cmp::Reverse<(tokio::time::Instant, Bytes, u64)>,
        > = std::collections::BinaryHeap::new();

        tokio::pin!(input);

        loop {
            let input_next = input.next();

            let next_timer_deadline = flush_deadlines
                .peek()
                .map(|&std::cmp::Reverse((d, _, _))| d);

            tokio::select! {
                event = input_next => {
                    match event {
                        Some(event) => {
                            let path = match partitioner.partition(&event) {
                                Some(raw_path) => {
                                    if let Some(ref confinement) = self.confinement {
                                        match confinement.confine(&bytes_to_path(&raw_path)) {
                                            Ok(confined) => {
                                                #[cfg(unix)]
                                                {
                                                    use std::os::unix::ffi::OsStrExt;
                                                    Bytes::copy_from_slice(
                                                        confined.as_os_str().as_bytes(),
                                                    )
                                                }
                                                #[cfg(not(unix))]
                                                {
                                                    Bytes::from(
                                                        confined
                                                            .to_string_lossy()
                                                            .as_bytes()
                                                            .to_vec(),
                                                    )
                                                }
                                            }
                                            Err(error) => {
                                                let rendered = bytes_to_path(&raw_path);
                                                let base = confinement.base_dir().to_path_buf();
                                                emit!(FilePathOutsideBaseDirError {
                                                    path: &rendered,
                                                    base_dir: &base,
                                                    error,
                                                    dropped_events: 1,
                                                });
                                                event.metadata()
                                                    .update_status(EventStatus::Errored);
                                                continue;
                                            }
                                        }
                                    } else {
                                        raw_path
                                    }
                                }
                                None => {
                                    event.metadata().update_status(EventStatus::Errored);
                                    continue;
                                }
                            };
                            let event_size = event.estimated_json_encoded_size_of().get();
                            if let Some((events, _generation, bytes)) = buffers.get_mut(&path) {
                                if *bytes + event_size > batch_settings.size_limit
                                    || events.len() >= batch_settings.item_limit
                                {
                                    // Buffer is full — flush old batch and start fresh.
                                    let (old_events, _old_generation, _old_bytes) =
                                        buffers.remove(&path).unwrap();
                                    self.process_batch(path.clone(), old_events).await;
                                    let generation = per_path_gen.entry(path.clone()).or_insert(0);
                                    let deadline = tokio::time::Instant::now()
                                        + batch_settings.timeout;
                                    buffers.insert(
                                        path.clone(),
                                        (vec![event], *generation, event_size),
                                    );
                                    flush_deadlines.push(
                                        std::cmp::Reverse((deadline, path.clone(), *generation)),
                                    );
                                    *generation += 1;
                                } else {
                                    *bytes += event_size;
                                    events.push(event);
                                }
                            } else {
                                let generation = per_path_gen.entry(path.clone()).or_insert(0);
                                let deadline = tokio::time::Instant::now()
                                    + batch_settings.timeout;
                                buffers.insert(
                                    path.clone(),
                                    (vec![event], *generation, event_size),
                                );
                                flush_deadlines.push(
                                    std::cmp::Reverse((deadline, path.clone(), *generation)),
                                );
                                *generation += 1;
                            }
                            // Flush immediately when the batch reaches the item or byte limit.
                            let needs_flush = buffers.get(&path).is_some_and(|(events, _, bytes)| {
                                *bytes >= batch_settings.size_limit
                                    || events.len() >= batch_settings.item_limit
                            });
                            if needs_flush {
                                let (events, _generation, _bytes) = buffers.remove(&path).unwrap();
                                self.process_batch(path.clone(), events).await;
                                // The stale deadline entry (generation) remains in the heap but won't
                                // match the new generation if this path receives more events.
                            }
                            // Bound active-buffer memory under high-cardinality templates.
                            // Also flush expired buffers inline.
                            {
                                let now = tokio::time::Instant::now();
                                loop {
                                    let expire_or_cap =
                                        flush_deadlines.peek().is_some_and(
                                            |std::cmp::Reverse((d, _, _))| {
                                                *d <= now || buffers.len() > 1000
                                            },
                                        );
                                    if !expire_or_cap {
                                        break;
                                    }
                                    let std::cmp::Reverse((_, path, generation)) =
                                        match flush_deadlines.pop() {
                                            Some(e) => e,
                                            None => break,
                                        };
                                    if let Some((events, current_generation, bytes)) =
                                        buffers.remove(&path)
                                    {
                                        if current_generation == generation {
                                            self.process_batch(path, events).await;
                                        } else {
                                            buffers.insert(path, (events, current_generation, bytes));
                                        }
                                    }
                                }
                            }
                        }
                        None => {
                            // Stream exhausted — flush all remaining buffers, then close files.
                            debug!(message = "Receiver exhausted, flushing remaining buffers.");
                            let paths: Vec<Bytes> = buffers.keys().cloned().collect();
                            for p in paths {
                                if let Some((events, _generation, _bytes)) = buffers.remove(&p) {
                                    self.process_batch(p, events).await;
                                }
                            }
                            debug!(message = "Closing all the open files.");
                            for (path, file) in self.files.iter_mut() {
                                if let Err(error) = file.close().await {
                                    emit!(FileIoError {
                                        error,
                                        code: "failed_closing_file",
                                        message: "Failed to close file.",
                                        path,
                                        dropped_events: 0,
                                    });
                                } else {
                                    trace!(message = "Successfully closed file.", path = ?path);
                                }
                            }
                            emit!(FileOpen { count: 0 });
                            break;
                        }
                    }
                }
                result = self.files.next_expired(), if !self.files.is_empty() => {
                    match result {
                        None => unreachable!(),
                        Some((expired_file, path)) => {
                            self.close_file(expired_file, path).await;
                        }
                    }
                }
                _ = async {
                    tokio::time::sleep_until(
                        next_timer_deadline
                            .unwrap_or_else(|| {
                                tokio::time::Instant::now()
                                    + std::time::Duration::from_secs(3600)
                            }),
                    ).await;
                }, if next_timer_deadline.is_some() => {}
            }

            // Flush any expired buffers after every wake-up.
            let now = tokio::time::Instant::now();
            loop {
                let expired = flush_deadlines
                    .peek()
                    .is_some_and(|std::cmp::Reverse((d, _, _))| *d <= now);
                if !expired {
                    break;
                }
                let std::cmp::Reverse((_, path, generation)) = match flush_deadlines.pop() {
                    Some(e) => e,
                    None => break,
                };
                if let Some((events, current_generation, bytes)) = buffers.remove(&path) {
                    if current_generation == generation {
                        self.process_batch(path, events).await;
                    } else {
                        buffers.insert(path, (events, current_generation, bytes));
                    }
                }
            }
        }

        Ok(())
    }

    async fn process_batch(&mut self, path: Bytes, mut events: Vec<Event>) {
        let next_deadline = self.deadline_at();
        trace!(message = "Computed next deadline.", next_deadline = ?next_deadline, path = ?path);

        let bytes_path = BytesPath::new(path.clone());
        let truncate = self.should_truncate(&bytes_path, &path).await;
        let compression = self.compression;

        let file = if !truncate {
            if let Some(file) = self.files.reset_at(&path, next_deadline) {
                trace!(message = "Working with an already opened file.", path = ?path);
                file
            } else {
                trace!(message = "Opening new file.", ?path);
                let file = match open_file(bytes_path, truncate, self.confinement.as_mut()).await {
                    Ok(file) => file,
                    Err(OpenError::Io(error)) => {
                        // We couldn't open the file for this event.
                        // Maybe other events will work though! Just log
                        // the error and skip this event.
                        let dropped_events = events.len();
                        emit!(FileIoError {
                            code: "failed_opening_file",
                            message: "Unable to open the file.",
                            error,
                            path: &path,
                            dropped_events,
                        });
                        events.iter_mut().for_each(|event| {
                            event.metadata().update_status(EventStatus::Errored);
                        });
                        return;
                    }
                    Err(OpenError::Confine(error)) => {
                        let rendered = bytes_to_path(&path);
                        let base = self
                            .confinement
                            .as_ref()
                            .map(|c| c.base_dir().to_path_buf())
                            .unwrap_or_default();
                        let dropped_events = events.len();
                        emit!(FilePathOutsideBaseDirError {
                            path: &rendered,
                            base_dir: &base,
                            error,
                            dropped_events,
                        });
                        events.iter_mut().for_each(|event| {
                            event.metadata().update_status(EventStatus::Errored);
                        });
                        return;
                    }
                };

                let outfile = OutFile::new(file, compression);
                self.files.insert_at(path.clone(), outfile, next_deadline);
                emit!(FileOpen {
                    count: self.files.len()
                });
                self.files.get_mut(&path).unwrap()
            }
        } else {
            trace!(message = "Opening new file (truncating).", ?path);
            let file = match open_file(bytes_path, truncate, self.confinement.as_mut()).await {
                Ok(file) => file,
                Err(OpenError::Io(error)) => {
                    let dropped_events = events.len();
                    emit!(FileIoError {
                        code: "failed_opening_file",
                        message: "Unable to open the file.",
                        error,
                        path: &path,
                        dropped_events,
                    });
                    events.iter_mut().for_each(|event| {
                        event.metadata().update_status(EventStatus::Errored);
                    });
                    return;
                }
                Err(OpenError::Confine(error)) => {
                    let rendered = bytes_to_path(&path);
                    let base = self
                        .confinement
                        .as_ref()
                        .map(|c| c.base_dir().to_path_buf())
                        .unwrap_or_default();
                    let dropped_events = events.len();
                    emit!(FilePathOutsideBaseDirError {
                        path: &rendered,
                        base_dir: &base,
                        error,
                        dropped_events,
                    });
                    events.iter_mut().for_each(|event| {
                        event.metadata().update_status(EventStatus::Errored);
                    });
                    return;
                }
            };

            let outfile = OutFile::new(file, compression);
            self.files.insert_at(path.clone(), outfile, next_deadline);
            emit!(FileOpen {
                count: self.files.len()
            });
            self.files.get_mut(&path).unwrap()
        };

        // Encode each event individually so we can write them one at a time.
        // This ensures that if a write fails partway through (e.g. ENOSPC),
        // events already written are acknowledged Delivered and only the
        // remaining events are retried, avoiding silent duplicates.
        let mut encoded: Vec<(BytesMut, EventFinalizers, JsonSize)> =
            Vec::with_capacity(events.len());

        trace!(message = "Encoding batch.", batch_size = events.len(), path = ?path);
        for mut event in events {
            let event_size = event.estimated_json_encoded_size_of();
            let finalizers = event.take_finalizers();
            self.transformer.transform(&mut event);
            let mut buf = BytesMut::new();
            match self.encoder.encode(event, &mut buf) {
                Ok(()) => encoded.push((buf, finalizers, event_size)),
                Err(error) => {
                    finalizers.update_status(EventStatus::Errored);
                    emit!(FileIoError {
                        code: "failed_encoding_event",
                        message: "Failed to encode event.",
                        error: std::io::Error::new(std::io::ErrorKind::InvalidData, error),
                        path: &path,
                        dropped_events: 1,
                    });
                }
            }
        }

        if encoded.is_empty() {
            return;
        }

        // Combine all encoded records into a single buffer so we issue one
        // write syscall per batch for the common uncompressed case.  Track
        // each event's byte boundary so a partial write (ENOSPC, quota) can
        // still acknowledge the events that were fully persisted.
        let n_events = encoded.len();
        let mut batch_buffer = BytesMut::new();
        let mut boundaries: Vec<usize> = Vec::with_capacity(n_events);
        for (buf, _, _) in &encoded {
            boundaries.push(batch_buffer.len() + buf.len());
            batch_buffer.extend_from_slice(buf);
        }

        let len = batch_buffer.len();
        if len == 0 {
            for (_, finalizers, event_size) in encoded {
                finalizers.update_status(EventStatus::Delivered);
                self.events_sent.emit(CountByteSize(1, event_size));
            }
            emit_bytes_sent(&path, 0, self.include_file_metric_tag);
            return;
        }

        file.mark_written();
        let file_start = match file.len().await {
            Ok(start) => start,
            Err(error) => {
                emit!(FileIoError {
                    code: "failed_writing_file",
                    message: "Failed to determine file length before writing.",
                    error,
                    path: &path,
                    dropped_events: n_events,
                });
                encoded.into_iter().for_each(|(_, finalizers, _)| {
                    finalizers.update_status(EventStatus::Errored);
                });
                return;
            }
        };
        let mut written = 0usize;
        let write_result: Result<(), std::io::Error> = loop {
            match file.write(&batch_buffer[written..]).await {
                Ok(0) => {
                    break Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "write returned 0",
                    ));
                }
                Ok(n) => {
                    written += n;
                    if written >= len {
                        break Ok(());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => break Err(e),
            }
        };

        match write_result {
            Ok(()) => {
                for (_buf, finalizers, event_size) in encoded {
                    finalizers.update_status(EventStatus::Delivered);
                    self.events_sent.emit(CountByteSize(1, event_size));
                }
                emit_bytes_sent(&path, len, self.include_file_metric_tag);
            }
            Err(error) => {
                // `written` bytes made it to the file / compression stream.
                // Events whose end offset lies at or before `written` were
                // fully persisted; everything beyond that must be retried.
                let last_complete = boundaries
                    .iter()
                    .rfind(|&&b| b <= written)
                    .copied()
                    .unwrap_or(0);
                let delivered_up_to = reconcile_after_partial_write(
                    file,
                    compression,
                    file_start,
                    written,
                    last_complete,
                    &boundaries,
                )
                .await;

                let dropped_events = boundaries.iter().filter(|&&b| b > delivered_up_to).count();
                for (i, (_buf, finalizers, event_size)) in encoded.into_iter().enumerate() {
                    if boundaries[i] <= delivered_up_to {
                        finalizers.update_status(EventStatus::Delivered);
                        self.events_sent.emit(CountByteSize(1, event_size));
                    } else {
                        finalizers.update_status(EventStatus::Errored);
                    }
                }
                emit!(FileIoError {
                    code: "failed_writing_file",
                    message: "Failed to write the file.",
                    error,
                    path: &path,
                    dropped_events,
                });
                // Telemetry counts only the bytes actually accepted by the
                // file/encoder; `delivered_up_to` may extend into a partial
                // event that was never accepted.
                let bytes_accepted = written.min(delivered_up_to);
                if bytes_accepted > 0 {
                    emit_bytes_sent(&path, bytes_accepted, self.include_file_metric_tag);
                }
            }
        }
    }

    async fn should_truncate(&mut self, bytes_path: &BytesPath, path: &bytes::Bytes) -> bool {
        let mut truncate = false;

        if let Some(after_close_time_secs) = self.truncation_config.after_close_time_secs
            && self.files.get(path).is_none()
            && let Ok(metadata) = fs::metadata(bytes_path).await
            && let Ok(time) = metadata
                .modified()
                .map_err(|_| ())
                .and_then(|t| t.elapsed().map_err(|_| ()))
            && time.as_secs() > after_close_time_secs.into()
        {
            truncate = true;
        }

        if let Some(after_secs) = self.truncation_config.after_secs
            && let Some(file) = self.files.get(path)
            && (file.created_at().elapsed().as_secs() > after_secs.into())
        {
            truncate = true;
        }

        if let Some(after_modified_time_secs) = self.truncation_config.after_modified_time_secs
            && let Some(previous_modification) = self
                .files
                .get_with_deadline(path)
                .and_then(|(_, deadline)| deadline.checked_sub(self.idle_timeout))
            && previous_modification.elapsed().as_secs() > after_modified_time_secs.into()
        {
            truncate = true;
        }

        if truncate && let Some((file, path)) = self.files.remove(path) {
            self.close_file(file, path).await;
        }

        truncate
    }

    async fn close_file(&self, mut file: OutFile, path: Expired<Bytes>) {
        if let Err(error) = file.close().await {
            emit!(FileIoError {
                error,
                code: "failed_closing_file",
                message: "Failed to close file.",
                path: &path,
                dropped_events: 0,
            });
        }
        drop(file); // ignore close error
        emit!(FileOpen {
            count: self.files.len()
        });
    }
}

#[cfg(unix)]
fn bytes_to_path(b: &Bytes) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(b))
}

#[cfg(not(unix))]
fn bytes_to_path(b: &Bytes) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(b).as_ref())
}

/// Errors produced by `open_file`. Routed at the call site so that
/// confinement failures emit `FilePathOutsideBaseDirError` (INTENTIONAL drop)
/// instead of the generic `FileIoError` (UNINTENTIONAL).
#[derive(Debug)]
enum OpenError {
    Io(std::io::Error),
    Confine(ConfineError),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Confine(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OpenError {}

/// Create `path` and all missing ancestors, refusing to follow any symlink
/// in the components that fall *below* `base`.
///
/// The `base` directory is operator-authored and trusted — it is created with
/// the standard `create_dir_all` (which follows symlinks), so paths like
/// `/tmp/myapp` work correctly on macOS where `/tmp → /private/tmp`.
/// Only the suffix of `path` below `base` — the event-controlled part —
/// is walked component-by-component with `lstat` checks.
///
/// A residual TOCTOU window exists between the `symlink_metadata` check and
/// the `create_dir` call. Closing it requires fd-based traversal (`cap-std`),
/// which is Phase 1b scope. `verify_parent` provides a second layer of
/// defence after this call.
#[cfg(unix)]
async fn create_dirs_nofollow(path: &Path, base: &Path) -> std::io::Result<()> {
    fs::create_dir_all(base).await?;
    let suffix = path.strip_prefix(base).unwrap_or(path);
    let mut current = base.to_path_buf();
    for component in suffix.components() {
        current.push(component);
        match fs::symlink_metadata(&current).await {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(std::io::Error::other(format!(
                    "intermediate path component {current:?} is a symlink"
                )));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current).await {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn emit_bytes_sent(path: &Bytes, byte_size: usize, include_file_metric_tag: bool) {
    emit!(FileBytesSent {
        byte_size,
        file: String::from_utf8_lossy(path),
        include_file_metric_tag,
    });
}

/// Rolls back a partially written batch to its last complete record,
/// returning the batch offset up to which events are acked.
async fn reconcile_after_partial_write(
    file: &mut OutFile,
    compression: Compression,
    file_start: u64,
    written: usize,
    last_complete: usize,
    boundaries: &[usize],
) -> usize {
    if last_complete >= written {
        return last_complete;
    }

    // The partial bytes can't be removed; ack it rather than retry
    // (a retry would re-write the retained prefix).
    let ack_partial = || {
        boundaries
            .iter()
            .find(|&&b| b > last_complete)
            .copied()
            .unwrap_or(written)
    };

    match compression {
        Compression::None => {
            let rolled_back = match file.len().await {
                Ok(current_len) => current_len == file_start + written as u64,
                Err(_) => false,
            };
            if rolled_back {
                let rewind_to = file_start + last_complete as u64;
                if let Err(e) = file.reset(rewind_to).await {
                    warn!(message = "Failed to rewind file after partial write.", error = ?e);
                    ack_partial()
                } else {
                    last_complete
                }
            } else {
                warn!(
                    message =
                        "File changed while writing; cannot safely rewind after partial write.",
                );
                ack_partial()
            }
        }
        Compression::Gzip | Compression::Zstd => {
            // Rewind only when the file still ends where this batch left it,
            // then complete the frame.
            let expected_end = file_start + file.written_bytes();
            let matches_expected_end = match file.len().await {
                Ok(actual_end) => actual_end == expected_end,
                Err(_) => false,
            };
            if !matches_expected_end {
                // The encoder still holds the accepted prefix and it can't be
                // removed (foreign data follows); ack the partial, don't retry.
                warn!(
                    message =
                        "File changed while writing; cannot safely rewind after partial write.",
                );
                return ack_partial();
            }
            if let Err(error) = file.finish_and_reopen().await {
                warn!(
                    message = "Failed to complete compression stream after partial write; rewinding best-effort.",
                    error = ?error,
                );
            }
            // The finished frame holds the partial prefix; drop the whole batch
            // before retrying, or ack the partial if it can't be removed.
            match file.reset(file_start).await {
                Ok(()) => 0,
                Err(e) => {
                    warn!(message = "Failed to rewind file after partial write.", error = ?e);
                    ack_partial()
                }
            }
        }
    }
}

async fn open_file(
    path: impl AsRef<Path>,
    truncate: bool,
    confinement: Option<&mut PathConfinement>,
) -> Result<File, OpenError> {
    let path_ref = path.as_ref();
    let parent = path_ref.parent();
    let file_name = path_ref.file_name();

    let confined = confinement.is_some();
    // Extract the base before `confinement` is moved into the open_path match.
    #[cfg(unix)]
    let base_dir = confinement.as_ref().map(|c| c.base_dir().to_path_buf());

    if let Some(parent) = parent {
        // When confined, refuse to follow intermediate symlinks in the
        // event-controlled portion of the path. On non-Unix platforms we fall
        // back to the standard `create_dir_all` (Windows reparse-point
        // protection is Phase 1b scope).
        #[cfg(unix)]
        if let Some(ref base) = base_dir {
            create_dirs_nofollow(parent, base)
                .await
                .map_err(OpenError::Io)?;
        } else {
            fs::create_dir_all(parent).await.map_err(OpenError::Io)?;
        }
        #[cfg(not(unix))]
        fs::create_dir_all(parent).await.map_err(OpenError::Io)?;
    }

    // If confined, verify the parent canonicalizes within the base, and
    // open relative to the canonicalized parent. This catches symlinks on
    // any intermediate directory.
    let open_path: PathBuf = match (confinement, parent, file_name) {
        (Some(confinement), Some(parent), Some(file_name)) => {
            let canonical_parent = confinement
                .verify_parent(parent)
                .await
                .map_err(OpenError::Confine)?;
            canonical_parent.join(file_name)
        }
        _ => path_ref.to_path_buf(),
    };

    let mut opts = fs::OpenOptions::new();
    opts.read(false)
        .write(true)
        .create(true)
        .append(!truncate)
        .truncate(truncate);

    // Reject final-component symlinks when confined. Do NOT apply
    // O_NOFOLLOW to unconfined static paths — operators who intentionally
    // symlink their log file (outside the threat model) must keep working.
    #[cfg(unix)]
    if confined {
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    let _ = confined;

    opts.open(open_path).await.map_err(OpenError::Io)
}

struct FilePathPartitioner {
    path: UnconfinedTemplate,
}

impl Partitioner for FilePathPartitioner {
    type Item = Event;
    type Key = Option<Bytes>;

    fn partition(&self, event: &Self::Item) -> Self::Key {
        match self.path.render(event) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("path"),
                    drop_event: true,
                });
                None
            }
        }
    }
}

#[async_trait]
impl StreamSink<Event> for FileSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        FileSink::run(&mut self, input)
            .await
            .expect("file sink error");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use chrono::{SubsecRound, Utc};
    use futures::{SinkExt, stream};
    #[cfg(unix)]
    use serial_test::serial;
    use similar_asserts::assert_eq;
    use vector_lib::{
        codecs::{JsonSerializerConfig, encoding::SerializerConfig},
        event::{LogEvent, TraceEvent},
        sink::VectorSink,
    };
    #[cfg(unix)]
    use vector_lib::{
        event::{EventArray, MetricValue},
        metrics::Controller,
    };
    use vrl::event_path;

    use super::*;
    use crate::{
        config::log_schema,
        test_util::{
            components::{FILE_SINK_TAGS, assert_sink_compliance},
            lines_from_file, lines_from_gzip_file, lines_from_zstd_file, random_events_with_stream,
            random_lines_with_stream, random_metrics_with_stream,
            random_metrics_with_stream_timestamp, temp_dir, temp_file, trace_init,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<FileSinkConfig>();
    }

    #[test]
    fn validate_produces_usable_state() {
        let config = base_config("/tmp/vector-test.log");
        let _validated = config.validate().expect("validation should succeed");
        // Serializer construction is deferred to `build`; validation retains the
        // transformer so `build` can construct the encoder.
        assert!(matches!(
            config.encoding.config().1,
            SerializerConfig::Text(_)
        ));
    }

    #[tokio::test]
    async fn log_single_partition() {
        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: FileInternalMetricsConfig {
                include_file_tag: true,
            },
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: Default::default(),
        };

        let (input, _events) = random_lines_with_stream(100, 64, None);

        run_assert_log_sink(&config, input.clone()).await;

        let output = lines_from_file(template);
        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[tokio::test]
    async fn log_single_partition_gzip() {
        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::Gzip,
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: FileInternalMetricsConfig {
                include_file_tag: true,
            },
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: Default::default(),
        };

        let (input, _) = random_lines_with_stream(100, 64, None);

        run_assert_log_sink(&config, input.clone()).await;

        let output = lines_from_gzip_file(template);
        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[tokio::test]
    async fn log_single_partition_zstd() {
        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::Zstd,
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: FileInternalMetricsConfig {
                include_file_tag: true,
            },
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: Default::default(),
        };

        let (input, _) = random_lines_with_stream(100, 64, None);

        run_assert_log_sink(&config, input.clone()).await;

        let output = lines_from_zstd_file(template);
        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[tokio::test]
    async fn log_many_partitions() {
        let directory = temp_dir();

        let mut template = directory.to_string_lossy().to_string();
        template.push_str("/{{level}}s-{{date}}.log");

        trace!(message = "Template.", %template);

        let config = FileSinkConfig {
            path: template.try_into().unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: FileInternalMetricsConfig {
                include_file_tag: true,
            },
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: Default::default(),
        };

        let (mut input, _events) = random_events_with_stream(32, 8, None);
        input[0]
            .as_mut_log()
            .insert(event_path!("date"), "2019-26-07");
        input[0]
            .as_mut_log()
            .insert(event_path!("level"), "warning");
        input[1]
            .as_mut_log()
            .insert(event_path!("date"), "2019-26-07");
        input[1].as_mut_log().insert(event_path!("level"), "error");
        input[2]
            .as_mut_log()
            .insert(event_path!("date"), "2019-26-07");
        input[2]
            .as_mut_log()
            .insert(event_path!("level"), "warning");
        input[3]
            .as_mut_log()
            .insert(event_path!("date"), "2019-27-07");
        input[3].as_mut_log().insert(event_path!("level"), "error");
        input[4]
            .as_mut_log()
            .insert(event_path!("date"), "2019-27-07");
        input[4]
            .as_mut_log()
            .insert(event_path!("level"), "warning");
        input[5]
            .as_mut_log()
            .insert(event_path!("date"), "2019-27-07");
        input[5]
            .as_mut_log()
            .insert(event_path!("level"), "warning");
        input[6]
            .as_mut_log()
            .insert(event_path!("date"), "2019-28-07");
        input[6]
            .as_mut_log()
            .insert(event_path!("level"), "warning");
        input[7]
            .as_mut_log()
            .insert(event_path!("date"), "2019-29-07");
        input[7].as_mut_log().insert(event_path!("level"), "error");

        run_assert_sink(&config, input.clone().into_iter()).await;

        let output = [
            lines_from_file(directory.join("warnings-2019-26-07.log")),
            lines_from_file(directory.join("errors-2019-26-07.log")),
            lines_from_file(directory.join("warnings-2019-27-07.log")),
            lines_from_file(directory.join("errors-2019-27-07.log")),
            lines_from_file(directory.join("warnings-2019-28-07.log")),
            lines_from_file(directory.join("errors-2019-29-07.log")),
        ];

        let message_key = log_schema().message_key().unwrap().to_string();
        assert_eq!(
            input[0].as_log()[&message_key],
            From::<&str>::from(&output[0][0])
        );
        assert_eq!(
            input[1].as_log()[&message_key],
            From::<&str>::from(&output[1][0])
        );
        assert_eq!(
            input[2].as_log()[&message_key],
            From::<&str>::from(&output[0][1])
        );
        assert_eq!(
            input[3].as_log()[&message_key],
            From::<&str>::from(&output[3][0])
        );
        assert_eq!(
            input[4].as_log()[&message_key],
            From::<&str>::from(&output[2][0])
        );
        assert_eq!(
            input[5].as_log()[&message_key],
            From::<&str>::from(&output[2][1])
        );
        assert_eq!(
            input[6].as_log()[&message_key],
            From::<&str>::from(&output[4][0])
        );
        assert_eq!(
            input[7].as_log()[message_key],
            From::<&str>::from(&output[5][0])
        );
    }

    #[tokio::test]
    async fn log_reopening() {
        trace_init();

        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout: Duration::from_secs(1),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: FileInternalMetricsConfig {
                include_file_tag: true,
            },
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: Default::default(),
        };

        let (mut input, _events) = random_lines_with_stream(10, 64, None);

        let (mut tx, rx) = futures::channel::mpsc::channel(0);

        let sink_handle = tokio::spawn(async move {
            assert_sink_compliance(&FILE_SINK_TAGS, async move {
                let sink = FileSink::new(&config, SinkContext::default()).unwrap();
                VectorSink::from_event_streamsink(sink)
                    .run(Box::pin(rx.map(Into::into)))
                    .await
                    .expect("Running sink failed");
            })
            .await
        });

        // send initial payload
        for line in input.clone() {
            tx.send(Event::Log(LogEvent::from(line))).await.unwrap();
        }

        // wait for file to go idle and be closed
        tokio::time::sleep(Duration::from_secs(2)).await;

        // trigger another write
        let last_line = "i should go at the end";
        tx.send(LogEvent::from(last_line).into()).await.unwrap();
        input.push(String::from(last_line));

        // wait for batch timeout (1s default) plus margin to flush
        tokio::time::sleep(Duration::from_secs(3)).await;

        // make sure we appended instead of overwriting
        let output = lines_from_file(template);
        assert_eq!(input, output);

        // make sure sink stops and that it did not panic
        drop(tx);
        sink_handle.await.unwrap();
    }

    #[tokio::test]
    async fn metric_single_partition() {
        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: FileInternalMetricsConfig {
                include_file_tag: true,
            },
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: Default::default(),
        };

        let (input, _events) = random_metrics_with_stream(100, None, None);

        run_assert_sink(&config, input.clone().into_iter()).await;

        let output = lines_from_file(template);
        for (input, output) in input.into_iter().zip(output) {
            let metric_name = input.as_metric().name();
            assert!(output.contains(metric_name));
        }
    }

    #[tokio::test]
    async fn metric_many_partitions() {
        let directory = temp_dir();

        let format = "%Y-%m-%d-%H-%M-%S";
        let mut template = directory.to_string_lossy().to_string();
        template.push_str(&format!("/{format}.log"));

        let config = FileSinkConfig {
            path: template.try_into().unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: FileInternalMetricsConfig {
                include_file_tag: true,
            },
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: Default::default(),
        };

        let metric_count = 3;
        let timestamp = Utc::now().trunc_subsecs(3);
        let timestamp_offset = Duration::from_secs(1);

        let (input, _events) = random_metrics_with_stream_timestamp(
            metric_count,
            None,
            None,
            timestamp,
            timestamp_offset,
        );

        run_assert_sink(&config, input.clone().into_iter()).await;

        let output = (0..metric_count).map(|index| {
            let expected_timestamp = timestamp + (timestamp_offset * index as u32);
            let expected_filename =
                directory.join(format!("{}.log", expected_timestamp.format(format)));

            lines_from_file(expected_filename)
        });
        for (input, output) in input.iter().zip(output) {
            // The format will partition by second and metrics are a second apart.
            assert_eq!(
                output.len(),
                1,
                "Expected the output file to contain one metric"
            );
            let output = &output[0];

            let metric_name = input.as_metric().name();
            assert!(output.contains(metric_name));
        }
    }

    #[tokio::test]
    async fn trace_single_partition() {
        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, JsonSerializerConfig::default()).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: FileInternalMetricsConfig {
                include_file_tag: true,
            },
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: Default::default(),
        };

        let (input, _events) = random_lines_with_stream(100, 64, None);

        run_assert_trace_sink(&config, input.clone()).await;

        let output = lines_from_file(template);
        for (input, output) in input.iter().zip(output) {
            assert!(output.contains(input));
        }
    }

    fn base_config(path: &str) -> FileSinkConfig {
        FileSinkConfig {
            path: path.try_into().unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: Default::default(),
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
            batch: BatchConfig::default(),
        }
    }

    // Uses Unix-shaped `/` absolute paths in test fixtures. On Windows those
    // strings aren't recognised as absolute and the build-error taxonomy
    // shifts (NoDerivableBase vs DerivedBaseIsRoot).
    #[cfg(unix)]
    #[test]
    fn sink_build_cases() {
        enum Expected {
            NoConfinement,
            Confined,
            ErrContaining(&'static str),
        }
        use Expected::*;
        let dir = temp_dir();
        let dynamic = format!("{}/{{{{ key }}}}.log", dir.display());
        let cases: &[(&str, Option<PathBuf>, bool, Expected)] = &[
            // static path → no confinement
            ("/tmp/static.log", None, false, NoConfinement),
            // dynamic path → confinement auto-derived
            (&dynamic, None, false, Confined),
            // no derivable base → error
            (
                "{{ key }}",
                None,
                false,
                ErrContaining("no literal directory prefix"),
            ),
            // derived base is root → error
            (
                "/{{ x }}/a.log",
                None,
                false,
                ErrContaining("filesystem root"),
            ),
            // explicit root base_dir is allowed (operator opt-in)
            ("/{{ x }}", Some(PathBuf::from("/")), false, Confined),
            // escape hatch suppresses NoDerivableBase
            ("{{ key }}", None, true, NoConfinement),
        ];
        for (path, base_dir, hatch, expected) in cases {
            let mut cfg = base_config(path);
            cfg.base_dir = base_dir.clone();
            cfg.confinement
                .dangerously_allow_unconfined_template_resolution = *hatch;
            let result = FileSink::new(&cfg, SinkContext::default());
            match expected {
                NoConfinement => assert!(result.unwrap().confinement.is_none(), "path={path:?}"),
                Confined => assert!(result.unwrap().confinement.is_some(), "path={path:?}"),
                ErrContaining(msg) => {
                    let err = match result {
                        Err(e) => e,
                        Ok(_) => panic!("expected build error for path={path:?}"),
                    };
                    assert!(err.to_string().contains(msg), "path={path:?} err={err}");
                }
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn confine_drops_dotdot_traversal() {
        // PoC payload: tenant field carries `../..` to escape the base dir.
        let dir = temp_dir();
        let apps = dir.join("apps");
        let template = format!("{}/{{{{ service }}}}/app.log", apps.display());
        let mut cfg = base_config(&template);
        cfg.base_dir = Some(apps.clone());

        let mut event = Event::Log(LogEvent::from("payload"));
        event
            .as_mut_log()
            .insert(event_path!("service"), "../../../etc/cron.d/vh-poc");

        // Run without compliance checks — no events are sent, so the compliance
        // metric assertions (BytesSent / component_sent_bytes_total) would fail.
        let sink = FileSink::new(&cfg, SinkContext::default()).unwrap();
        VectorSink::from_event_streamsink(sink)
            .run(Box::pin(stream::iter(
                vec![event].into_iter().map(Into::into),
            )))
            .await
            .expect("Running sink failed");

        // `confine` rejects the traversal before any filesystem mutation.
        assert!(
            !apps.exists(),
            "base_dir should not have been created: {apps:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn confine_collapses_absolute_injection_into_base() {
        // When a field value begins with `/`, the template render produces
        // `<base>//<value>` which lexically collapses to `<base>/<value>`.
        // The leading slash is harmless — the event is still confined to the base.
        let dir = temp_dir();
        let template = format!("{}/{{{{ key }}}}.log", dir.display());
        let mut cfg = base_config(&template);
        cfg.base_dir = Some(dir.clone());
        cfg.internal_metrics = FileInternalMetricsConfig {
            include_file_tag: true,
        };

        let mut event = Event::Log(LogEvent::from("payload"));
        event.as_mut_log().insert(event_path!("key"), "/etc/passwd");

        run_assert_sink(&cfg, vec![event].into_iter()).await;

        let expected = dir.join("etc/passwd.log");
        assert!(
            expected.exists(),
            "expected {expected:?} to exist under base {}",
            dir.display()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn confine_allows_legit_partition() {
        let dir = temp_dir();
        let template = format!("{}/{{{{ key }}}}.log", dir.display());
        let mut cfg = base_config(&template);
        cfg.base_dir = Some(dir.clone());
        cfg.internal_metrics = FileInternalMetricsConfig {
            include_file_tag: true,
        };

        let mut event = Event::Log(LogEvent::from("payload"));
        event.as_mut_log().insert(event_path!("key"), "tenant-a");

        run_assert_sink(&cfg, vec![event].into_iter()).await;

        let expected = dir.join("tenant-a.log");
        assert!(expected.exists(), "expected file not created: {expected:?}");
    }

    #[test]
    fn escape_hatch_suppresses_build_error() {
        // No base derivable, but the flag is set → build succeeds with
        // confinement disabled.
        let mut cfg = base_config("{{ key }}");
        cfg.confinement
            .dangerously_allow_unconfined_template_resolution = true;
        let sink = FileSink::new(&cfg, SinkContext::default()).unwrap();
        assert!(sink.confinement.is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn escape_hatch_bypasses_confinement_even_when_base_derivable() {
        // With the flag set, confinement is fully disabled — even when a base
        // would otherwise be derivable. The flag is a complete opt-out.
        let dir = temp_dir();
        let template = format!("{}/{{{{ key }}}}.log", dir.display());
        let mut cfg = base_config(&template);
        cfg.confinement
            .dangerously_allow_unconfined_template_resolution = true;
        cfg.internal_metrics = FileInternalMetricsConfig {
            include_file_tag: true,
        };

        let mut event = Event::Log(LogEvent::from("payload"));
        event.as_mut_log().insert(event_path!("key"), "safe-value");

        run_assert_sink(&cfg, vec![event].into_iter()).await;

        let expected = dir.join("safe-value.log");
        assert!(expected.exists(), "expected file not created: {expected:?}");
    }

    // Regression test: a batch rejected by the open-time confinement check must
    // count every event. The base is swapped for a symlink after the first open
    // caches it.
    #[cfg(unix)]
    #[test]
    #[serial]
    fn open_time_confinement_rejection_counts_whole_batch() {
        trace_init();
        use tempfile::tempdir;

        let before = discarded_intentional_count();
        // A current-thread runtime keeps counter increments in this thread's registry.
        let current_thread = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        current_thread.block_on(async {
            let tmp = tempdir().unwrap();
            let outside = tmp.path().join("outside");
            tokio::fs::create_dir(&outside).await.unwrap();

            let base = tmp.path().join("base");
            tokio::fs::create_dir(&base).await.unwrap();

            let template = format!("{}/{{{{ key }}}}/app.log", base.display());
            let mut cfg = base_config(&template);
            cfg.base_dir = Some(base.clone());

            let (tx, rx) = futures::channel::mpsc::unbounded::<EventArray>();
            let sink = FileSink::new(&cfg, SinkContext::default()).unwrap();
            let handle = tokio::spawn(async move {
                VectorSink::from_event_streamsink(sink)
                    .run(rx)
                    .await
                    .expect("running file sink failed")
            });

            // Wait for the first open, which caches the canonicalized base.
            let mut seed = Event::Log(LogEvent::from("seed"));
            seed.as_mut_log().insert(event_path!("key"), "a");
            tx.unbounded_send(EventArray::from(seed)).unwrap();
            let seeded = base.join("a/app.log");
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if tokio::fs::try_exists(&seeded).await.unwrap_or(false) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("seed open did not complete in time");

            let relocated = tmp.path().join("relocated");
            tokio::fs::rename(&base, &relocated).await.unwrap();
            tokio::fs::symlink(&outside, &base).await.unwrap();

            let rejected = base.join("b/app.log");
            for i in 0..3 {
                let mut event = Event::Log(LogEvent::from(format!("payload {i}")));
                event.as_mut_log().insert(event_path!("key"), "b");
                tx.unbounded_send(EventArray::from(event)).unwrap();
            }
            drop(tx);
            handle.await.expect("sink task panicked");

            assert!(
                !tokio::fs::try_exists(&rejected).await.unwrap(),
                "rejected batch must not be written to the confined path"
            );
            assert!(
                !tokio::fs::try_exists(&outside.join("b/app.log"))
                    .await
                    .unwrap(),
                "jailbreak path must not be written"
            );
        });

        let after = discarded_intentional_count();
        assert_eq!(
            after - before,
            3.0,
            "every event rejected by the open-time confinement check must be counted"
        );
    }

    #[tokio::test]
    async fn vector_validate_no_fs_io() {
        // base_dir need not exist for FileSink::new to succeed:
        // we defer FS I/O until the first event, so `vector validate` works
        // on volumes that aren't mounted yet.
        let dir = temp_dir();
        let path = format!("{}/{{{{ key }}}}.log", dir.display());
        let mut cfg = base_config(&path);
        cfg.base_dir = Some(dir.join("does-not-yet-exist"));
        // No filesystem precondition is established.
        let _ = FileSink::new(&cfg, SinkContext::default()).unwrap();
    }

    async fn run_assert_log_sink(config: &FileSinkConfig, events: Vec<String>) {
        run_assert_sink(
            config,
            events.into_iter().map(LogEvent::from).map(Event::Log),
        )
        .await;
    }

    async fn run_assert_trace_sink(config: &FileSinkConfig, events: Vec<String>) {
        run_assert_sink(
            config,
            events
                .into_iter()
                .map(LogEvent::from)
                .map(TraceEvent::from)
                .map(Event::Trace),
        )
        .await;
    }

    async fn run_assert_sink(config: &FileSinkConfig, events: impl Iterator<Item = Event> + Send) {
        assert_sink_compliance(&FILE_SINK_TAGS, async move {
            let sink = FileSink::new(config, SinkContext::default()).unwrap();
            VectorSink::from_event_streamsink(sink)
                .run(Box::pin(stream::iter(events.map(Into::into))))
                .await
                .expect("Running sink failed")
        })
        .await;
    }

    #[cfg(unix)]
    fn discarded_intentional_count() -> f64 {
        Controller::get()
            .expect("metrics controller initialized")
            .capture_metrics()
            .into_iter()
            .find(|m| {
                m.name() == "component_discarded_events_total"
                    && m.tags()
                        .is_some_and(|t| t.get("intentional") == Some("true"))
            })
            .map(|m| match m.value() {
                MetricValue::Counter { value } => *value,
                other => panic!("expected counter for discarded events, got {other:?}"),
            })
            .unwrap_or(0.0)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn create_dirs_nofollow_rejects_intermediate_symlink() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let outside = tmp.path().join("outside");
        tokio::fs::create_dir(&outside).await.unwrap();

        let base = tmp.path().join("base");
        tokio::fs::create_dir(&base).await.unwrap();
        let link = base.join("link");
        tokio::fs::symlink(&outside, &link).await.unwrap();

        let result = create_dirs_nofollow(&link.join("newdir"), &base).await;
        assert!(result.is_err(), "expected error, got {result:?}");

        let mut rd = tokio::fs::read_dir(&outside).await.unwrap();
        assert!(
            rd.next_entry().await.unwrap().is_none(),
            "outside was mutated"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn create_dirs_nofollow_allows_system_symlinks_above_base() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let real_dir = tmp.path().join("real");
        tokio::fs::create_dir(&real_dir).await.unwrap();
        let sym_dir = tmp.path().join("sym");
        tokio::fs::symlink(&real_dir, &sym_dir).await.unwrap();

        // base sits under a symlink (like /tmp on macOS)
        let base = sym_dir.join("base");
        let path = base.join("sub");

        let result = create_dirs_nofollow(&path, &base).await;
        assert!(
            result.is_ok(),
            "should succeed through system symlink: {result:?}"
        );
    }

    // Simulates `reconcile_after_partial_write`'s compressed recovery; earlier
    // bytes may still be buffered in the encoder.
    async fn assert_preserved_completed_stream(compression: Compression, template: PathBuf) {
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&template)
            .await
            .unwrap();
        let mut out = OutFile::new(file, compression);

        out.write(b"hello\n").await.unwrap();
        out.write(b"world\n").await.unwrap();
        let on_disk_before = out.len().await.unwrap();

        out.finish_and_reopen().await.unwrap();

        assert!(
            out.len().await.unwrap() > on_disk_before,
            "completed stream must be flushed to disk, not discarded"
        );

        out.write(b"next\n").await.unwrap();
        out.close().await.unwrap();

        let expected = vec!["hello".to_owned(), "world".to_owned(), "next".to_owned()];
        let output = match compression {
            Compression::Gzip => lines_from_gzip_file(template),
            Compression::Zstd => lines_from_zstd_file(template),
            Compression::None => unreachable!("compressed stream expected"),
        };
        assert_eq!(output, expected);
    }

    #[tokio::test]
    async fn finish_and_reopen_preserves_completed_gzip_stream() {
        trace_init();
        assert_preserved_completed_stream(Compression::Gzip, temp_file()).await;
    }

    #[tokio::test]
    async fn finish_and_reopen_preserves_completed_zstd_stream() {
        trace_init();
        assert_preserved_completed_stream(Compression::Zstd, temp_file()).await;
    }

    // Regression test: expected-end validation refuses to roll back when
    // another writer appended.
    #[tokio::test]
    async fn reconcile_preserves_concurrent_appender_compressed() {
        trace_init();
        for compression in [Compression::Gzip, Compression::Zstd] {
            let template = temp_file();
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&template)
                .await
                .unwrap();
            let mut out = OutFile::new(file, compression);

            // Mark before sampling `file_start`.
            out.mark_written();
            let file_start = out.len().await.unwrap();

            // The batch's compressed bytes may still be buffered in the encoder.
            let payload = b"this batch belongs to this sink";
            let _ = out.write(payload).await.unwrap();

            // A concurrent writer appends before the rollback check.
            let mut other = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&template)
                .await
                .unwrap();
            tokio::io::AsyncWriteExt::write_all(&mut other, b"EXTRA")
                .await
                .unwrap();
            other.sync_all().await.unwrap();
            let raw_now = tokio::fs::read(&template).await.unwrap();
            assert_eq!(raw_now, b"EXTRA", "concurrent append must be visible");
            drop(other);

            let delivered_up_to =
                reconcile_after_partial_write(&mut out, compression, file_start, 4, 2, &[2, 8])
                    .await;
            // Ack the retained partial (end boundary 8) so it isn't retried.
            assert_eq!(delivered_up_to, 8);

            assert_eq!(out.len().await.unwrap(), file_start + b"EXTRA".len() as u64);
            let raw = tokio::fs::read(&template).await.unwrap();
            assert!(
                raw.ends_with(b"EXTRA"),
                "concurrent append must be preserved"
            );
        }
    }

    // A failed rollback leaves the partial bytes on disk; ack, don't retry.
    #[tokio::test]
    async fn uncompressed_failed_rollback_acks_partial_record() {
        trace_init();
        // A read-only fd makes `set_len` fail during rollback.
        let template = temp_file();
        tokio::fs::write(&template, b"0123456789").await.unwrap();
        let readonly = tokio::fs::File::open(&template).await.unwrap();
        let mut out = OutFile::new(readonly, Compression::None);

        let file_start = 6; // out.len() (10) minus written (4)
        let written = 4;
        let last_complete = 2;
        let boundaries = [last_complete, 5];
        assert_eq!(out.len().await.unwrap(), file_start + written as u64);

        let delivered_up_to = reconcile_after_partial_write(
            &mut out,
            Compression::None,
            file_start,
            written,
            last_complete,
            &boundaries,
        )
        .await;
        // Ack the partial record so it isn't retried.
        assert_eq!(delivered_up_to, 5);
        // The failed rollback left the file untouched.
        assert_eq!(out.len().await.unwrap(), file_start + written as u64);
        let raw = tokio::fs::read(&template).await.unwrap();
        assert_eq!(raw, b"0123456789");
    }

    // A concurrent appender leaves the partial prefix on disk; ack, don't retry.
    #[tokio::test]
    async fn uncompressed_concurrent_appender_acks_partial_record() {
        trace_init();
        let template = temp_file();
        tokio::fs::write(&template, b"0123456789").await.unwrap();
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&template)
            .await
            .unwrap();
        let mut out = OutFile::new(file, Compression::None);

        // The batch leaves the file at file_start + written.
        let file_start = 6; // out.len() (10) minus written (4)
        let written = 4;
        let last_complete = 2;
        let boundaries = [last_complete, 5];
        assert_eq!(out.len().await.unwrap(), file_start + written as u64);

        // A concurrent writer appends before the rollback check.
        let mut other = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&template)
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::write_all(&mut other, b"EXTRA")
            .await
            .unwrap();
        other.sync_all().await.unwrap();
        drop(other);

        let delivered_up_to = reconcile_after_partial_write(
            &mut out,
            Compression::None,
            file_start,
            written,
            last_complete,
            &boundaries,
        )
        .await;
        // Ack the partial record so it isn't retried.
        assert_eq!(delivered_up_to, 5);
        // No rewind happened; the concurrent append is preserved.
        assert_eq!(
            out.len().await.unwrap(),
            file_start + written as u64 + b"EXTRA".len() as u64
        );
        let raw = tokio::fs::read(&template).await.unwrap();
        assert!(
            raw.ends_with(b"EXTRA"),
            "concurrent append must be preserved"
        );
    }

    // A completed frame holds the partial prefix; the whole batch is truncated
    // before retrying so the retry can't duplicate the prefix.
    #[tokio::test]
    async fn compressed_completed_frame_removes_batch_before_retry() {
        trace_init();
        for compression in [Compression::Gzip, Compression::Zstd] {
            let template = temp_file();
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&template)
                .await
                .unwrap();
            let mut out = OutFile::new(file, compression);

            out.mark_written();
            let file_start = out.len().await.unwrap();
            let _ = out.write(b"AAAABBBB").await.unwrap();

            let delivered_up_to =
                reconcile_after_partial_write(&mut out, compression, file_start, 4, 2, &[2, 8])
                    .await;
            assert_eq!(delivered_up_to, 0, "entire batch must be retried");
            assert_eq!(
                out.len().await.unwrap(),
                file_start,
                "the batch's compressed bytes must be removed"
            );
        }
    }

    // When the file can't be rewound (readonly fd), the compressed partial is
    // acked rather than retried over its retained prefix.
    #[tokio::test]
    async fn compressed_failed_rewind_acks_partial_record() {
        trace_init();
        for compression in [Compression::Gzip, Compression::Zstd] {
            let template = temp_file();
            tokio::fs::write(&template, b"SEED").await.unwrap();
            let seed_len = tokio::fs::metadata(&template).await.unwrap().len();
            let readonly = tokio::fs::File::open(&template).await.unwrap();
            let mut out = OutFile::new(readonly, compression);

            out.mark_written();
            let file_start = out.len().await.unwrap();
            assert_eq!(file_start, seed_len);
            // The encoder buffers the input; nothing reaches the readonly file.
            let _ = out.write(b"AAAABBBB").await.unwrap();

            let delivered_up_to =
                reconcile_after_partial_write(&mut out, compression, file_start, 4, 2, &[2, 8])
                    .await;
            assert_eq!(delivered_up_to, 8, "partial record must be acked");
            assert_eq!(out.len().await.unwrap(), seed_len, "no bytes appended");
            let raw = tokio::fs::read(&template).await.unwrap();
            assert_eq!(raw, b"SEED");
        }
    }
}
