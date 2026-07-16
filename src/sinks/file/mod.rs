use std::{
    convert::TryFrom,
    num::NonZeroU64,
    path::{Path, PathBuf},
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
    io::AsyncWriteExt,
};
use tokio_util::{codec::Encoder as _, time::delay_queue::Expired};
use vector_lib::{
    EstimatedJsonEncodedSizeOf, TimeZone,
    codecs::{
        TextSerializerConfig,
        encoding::{Framer, FramingConfig},
    },
    configurable::configurable_component,
    internal_event::{CountByteSize, EventsSent, InternalEventHandle as _, Output, Registered},
};

use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType, Transformer},
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    event::{Event, EventStatus, Finalizable},
    expiring_hash_map::ExpiringHashMap,
    internal_events::{
        FileBytesSent, FileInternalMetricsConfig, FileIoError, FileOpen,
        FilePathOutsideBaseDirError, TemplateRenderingError,
    },
    sinks::util::{
        StreamSink,
        path_confinement::{ConfineError, PathConfinement},
        timezone_to_offset,
    },
    template::{ConfinementConfig, Template},
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
    #[configurable(metadata(docs::examples = "/tmp/vector-%Y-%m-%d.log"))]
    #[configurable(metadata(
        docs::examples = "/tmp/application-{{ application_id }}-%Y-%m-%d.log"
    ))]
    #[configurable(metadata(docs::examples = "/tmp/vector-%Y-%m-%d.log.zst"))]
    #[configurable(metadata(
        docs::warnings = "Rendered paths are confined to `base_dir` (derived from the literal prefix of `path` when unset). See the `base_dir` option."
    ))]
    pub path: Template,

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

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub timezone: Option<TimeZone>,

    #[configurable(derived)]
    #[serde(default)]
    pub internal_metrics: FileInternalMetricsConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub truncate: FileTruncateConfig,
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
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            path: Template::try_from("/tmp/vector-%Y-%m-%d.log").unwrap(),
            idle_timeout: default_idle_timeout(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Default::default(),
            acknowledgements: Default::default(),
            timezone: Default::default(),
            internal_metrics: Default::default(),
            truncate: Default::default(),
            base_dir: None,
            confinement: ConfinementConfig::default(),
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

struct OutFile {
    created_at: Instant,
    inner: OutFileInner,
}

enum OutFileInner {
    Regular(File),
    Gzip(GzipEncoder<File>),
    Zstd(ZstdEncoder<File>),
}

impl OutFile {
    fn new(file: File, compression: Compression) -> Self {
        Self {
            created_at: Instant::now(),
            inner: match compression {
                Compression::None => OutFileInner::Regular(file),
                Compression::Gzip => OutFileInner::Gzip(GzipEncoder::new(file)),
                Compression::Zstd => OutFileInner::Zstd(ZstdEncoder::new(file)),
            },
        }
    }

    async fn sync_all(&mut self) -> Result<(), std::io::Error> {
        match &mut self.inner {
            OutFileInner::Regular(file) => file.sync_all().await,
            OutFileInner::Gzip(gzip) => gzip.get_mut().sync_all().await,
            OutFileInner::Zstd(zstd) => zstd.get_mut().sync_all().await,
        }
    }

    async fn shutdown(&mut self) -> Result<(), std::io::Error> {
        match &mut self.inner {
            OutFileInner::Regular(file) => file.shutdown().await,
            OutFileInner::Gzip(gzip) => gzip.shutdown().await,
            OutFileInner::Zstd(zstd) => zstd.shutdown().await,
        }
    }

    async fn write_all(&mut self, src: &[u8]) -> Result<(), std::io::Error> {
        match &mut self.inner {
            OutFileInner::Regular(file) => file.write_all(src).await,
            OutFileInner::Gzip(gzip) => gzip.write_all(src).await,
            OutFileInner::Zstd(zstd) => zstd.write_all(src).await,
        }
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
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = FileSink::new(self, cx)?;
        self.confinement.set_confinement_gauge("sink", Self::NAME);
        Ok((
            super::VectorSink::from_event_streamsink(sink),
            future::ok(()).boxed(),
        ))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

pub struct FileSink {
    path: Template,
    transformer: Transformer,
    encoder: Encoder<Framer>,
    idle_timeout: Duration,
    files: ExpiringHashMap<Bytes, OutFile>,
    compression: Compression,
    events_sent: Registered<EventsSent>,
    include_file_metric_tag: bool,
    truncation_config: FileTruncateConfig,
    confinement: Option<PathConfinement>,
}

impl FileSink {
    pub fn new(config: &FileSinkConfig, cx: SinkContext) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let (framer, serializer) = config.encoding.build(SinkType::StreamBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

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

        Ok(Self {
            path: config.path.clone().with_tz_offset(offset),
            transformer,
            encoder,
            idle_timeout: config.idle_timeout,
            files: ExpiringHashMap::default(),
            compression: config.compression,
            events_sent: register!(EventsSent::from(Output(None))),
            include_file_metric_tag: config.internal_metrics.include_file_tag,
            truncation_config: config.truncate.clone(),
            confinement,
        })
    }

    /// Uses pass the `event` to `self.path` template to obtain the file path
    /// to store the event as.
    fn partition_event(&mut self, event: &Event) -> Option<bytes::Bytes> {
        let bytes = match self.path.render(event) {
            Ok(b) => b,
            Err(error) => {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("path"),
                    drop_event: true,
                });
                return None;
            }
        };

        if let Some(confinement) = self.confinement.as_ref() {
            let rendered_path = bytes_to_path(&bytes);
            match confinement.confine(&rendered_path) {
                Ok(normalized) => Some(path_to_bytes(&normalized)),
                Err(error) => {
                    emit!(FilePathOutsideBaseDirError {
                        path: &rendered_path,
                        base_dir: confinement.base_dir(),
                        error,
                    });
                    None
                }
            }
        } else {
            Some(bytes)
        }
    }

    fn deadline_at(&self) -> Instant {
        Instant::now()
            .checked_add(self.idle_timeout)
            .expect("unable to compute next deadline")
    }

    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> crate::Result<()> {
        loop {
            tokio::select! {
                event = input.next() => {
                    match event {
                        Some(event) => self.process_event(event).await,
                        None => {
                            // If we got `None` - terminate the processing.
                            debug!(message = "Receiver exhausted, terminating the processing loop.");

                            // Close all the open files.
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
                                } else{
                                    trace!(message = "Successfully closed file.", path = ?path);
                                }
                            }

                            emit!(FileOpen {
                                count: 0
                            });

                            break;
                        }
                    }
                }
                result = self.files.next_expired(), if !self.files.is_empty() => {
                    match result {
                        // We do not poll map when it's empty, so we should
                        // never reach this branch.
                        None => unreachable!(),
                        Some((expired_file, path)) => {
                            // We got an expired file. All we really want is to
                            // flush and close it.
                            self.close_file(expired_file, path).await;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn process_event(&mut self, mut event: Event) {
        let path = match self.partition_event(&event) {
            Some(path) => path,
            None => {
                // We weren't able to find the path to use for the
                // file.
                // The error is already handled at `partition_event`, so
                // here we just skip the event.
                event.metadata().update_status(EventStatus::Errored);
                return;
            }
        };

        let next_deadline = self.deadline_at();
        trace!(message = "Computed next deadline.", next_deadline = ?next_deadline, path = ?path);

        let bytes_path = BytesPath::new(path.clone());
        let truncate = self.should_truncate(&bytes_path, &path).await;
        let file = if !truncate && let Some(file) = self.files.reset_at(&path, next_deadline) {
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
                    emit!(FileIoError {
                        code: "failed_opening_file",
                        message: "Unable to open the file.",
                        error,
                        path: &path,
                        dropped_events: 1,
                    });
                    event.metadata().update_status(EventStatus::Errored);
                    return;
                }
                Err(OpenError::Confine(error)) => {
                    let rendered = bytes_to_path(&path);
                    let base = self
                        .confinement
                        .as_ref()
                        .map(|c| c.base_dir().to_path_buf())
                        .unwrap_or_default();
                    emit!(FilePathOutsideBaseDirError {
                        path: &rendered,
                        base_dir: &base,
                        error,
                    });
                    event.metadata().update_status(EventStatus::Errored);
                    return;
                }
            };

            let outfile = OutFile::new(file, self.compression);

            self.files.insert_at(path.clone(), outfile, next_deadline);
            emit!(FileOpen {
                count: self.files.len()
            });
            self.files.get_mut(&path).unwrap()
        };

        trace!(message = "Writing an event to file.", path = ?path);
        let event_size = event.estimated_json_encoded_size_of();
        let finalizers = event.take_finalizers();
        match write_event_to_file(file, event, &self.transformer, &mut self.encoder).await {
            Ok(byte_size) => {
                finalizers.update_status(EventStatus::Delivered);
                self.events_sent.emit(CountByteSize(1, event_size));
                emit!(FileBytesSent {
                    byte_size,
                    file: String::from_utf8_lossy(&path),
                    include_file_metric_tag: self.include_file_metric_tag,
                });
            }
            Err(error) => {
                finalizers.update_status(EventStatus::Errored);
                emit!(FileIoError {
                    code: "failed_writing_file",
                    message: "Failed to write the file.",
                    error,
                    path: &path,
                    dropped_events: 1,
                });
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

#[cfg(unix)]
fn path_to_bytes(p: &Path) -> Bytes {
    use std::os::unix::ffi::OsStrExt;
    Bytes::copy_from_slice(p.as_os_str().as_bytes())
}

#[cfg(not(unix))]
fn path_to_bytes(p: &Path) -> Bytes {
    Bytes::from(p.to_string_lossy().into_owned().into_bytes())
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
                    "intermediate path component {:?} is a symlink",
                    current
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

async fn write_event_to_file(
    file: &mut OutFile,
    mut event: Event,
    transformer: &Transformer,
    encoder: &mut Encoder<Framer>,
) -> Result<usize, std::io::Error> {
    transformer.transform(&mut event);
    let mut buffer = BytesMut::new();
    encoder
        .encode(event, &mut buffer)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    file.write_all(&buffer).await.map(|()| buffer.len())
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
    use similar_asserts::assert_eq;
    use vector_lib::{
        codecs::JsonSerializerConfig,
        event::{LogEvent, TraceEvent},
        sink::VectorSink,
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

        // wait for another flush
        tokio::time::sleep(Duration::from_secs(1)).await;

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

    #[tokio::test]
    async fn confine_drops_dotdot_traversal() {
        // PoC payload: tenant field carries `../..` to escape the base dir.
        let dir = temp_dir();
        let path = format!("{}/apps/{{{{ service }}}}/app.log", dir.display());
        let cfg = base_config(&path);

        let mut event = Event::Log(LogEvent::from("payload"));
        event
            .as_mut_log()
            .insert(event_path!("service"), "../../../etc/cron.d/vh-poc");

        let mut sink = FileSink::new(&cfg, SinkContext::default()).unwrap();
        assert!(sink.partition_event(&event).is_none());
    }

    #[tokio::test]
    async fn confine_collapses_absolute_injection_into_base() {
        // When a field value begins with `/`, the template render produces
        // `<base>//<value>` which lexically collapses to `<base>/<value>`.
        // The leading slash is harmless (a separator, not an escape) — the
        // event is still confined to the base.
        let dir = temp_dir();
        let path = format!("{}/{{{{ key }}}}.log", dir.display());
        let cfg = base_config(&path);

        let mut event = Event::Log(LogEvent::from("payload"));
        event.as_mut_log().insert(event_path!("key"), "/etc/passwd");

        let mut sink = FileSink::new(&cfg, SinkContext::default()).unwrap();
        let confined = sink.partition_event(&event).unwrap();
        let confined_str = String::from_utf8_lossy(&confined);
        assert!(
            confined_str.starts_with(&*dir.to_string_lossy()),
            "expected {confined_str} to remain under {}",
            dir.display()
        );
    }

    // The path template embeds a literal `/` before the field, which is
    // Unix-shaped: on Windows the rendered separator flips to `\`.
    #[cfg(unix)]
    #[tokio::test]
    async fn confine_allows_legit_partition() {
        let dir = temp_dir();
        let path = format!("{}/{{{{ key }}}}.log", dir.display());
        let cfg = base_config(&path);

        let mut event = Event::Log(LogEvent::from("payload"));
        event.as_mut_log().insert(event_path!("key"), "tenant-a");

        let mut sink = FileSink::new(&cfg, SinkContext::default()).unwrap();
        let rendered = sink.partition_event(&event).unwrap();
        let rendered_str = String::from_utf8_lossy(&rendered);
        assert!(rendered_str.ends_with("/tenant-a.log"), "{rendered_str}");
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

    #[tokio::test]
    async fn escape_hatch_bypasses_confinement_even_when_base_derivable() {
        // With the flag set, confinement is fully disabled — even when a base
        // would otherwise be derivable. The flag is a complete opt-out.
        let dir = temp_dir();
        let path = format!("{}/{{{{ key }}}}.log", dir.display());
        let mut cfg = base_config(&path);
        cfg.confinement
            .dangerously_allow_unconfined_template_resolution = true;

        let mut sink = FileSink::new(&cfg, SinkContext::default()).unwrap();
        assert!(sink.confinement.is_none());

        let mut event = Event::Log(LogEvent::from("payload"));
        event.as_mut_log().insert(event_path!("key"), "safe-value");
        // Event routes through — no confinement check.
        assert!(sink.partition_event(&event).is_some());
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
}
