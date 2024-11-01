use std::{convert::TryInto, future, path::PathBuf, time::Duration};

use bytes::Bytes;
use chrono::Utc;
use futures::{FutureExt, Stream, StreamExt, TryFutureExt};
use regex::bytes::Regex;
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use tokio::{sync::oneshot, task::spawn_blocking};
use tracing::{Instrument, Span};
use vector_lib::codecs::{BytesDeserializer, BytesDeserializerConfig};
use vector_lib::configurable::configurable_component;
use vector_lib::file_source::{
    calculate_ignore_before,
    paths_provider::glob::{Glob, MatchOptions},
    Checkpointer, FileFingerprint, FileServer, FingerprintStrategy, Fingerprinter, Line, ReadFrom,
    ReadFromConfig,
};
use vector_lib::finalizer::OrderedFinalizer;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path, path, OwnedValuePath};
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    EstimatedJsonEncodedSizeOf,
};
use vrl::value::Kind;

use super::util::{EncodingConfig, MultilineConfig};
use crate::{
    config::{
        log_schema, DataType, SourceAcknowledgementsConfig, SourceConfig, SourceContext,
        SourceOutput,
    },
    encoding_transcode::{Decoder, Encoder},
    event::{BatchNotifier, BatchStatus, LogEvent},
    internal_events::{
        FileBytesReceived, FileEventsReceived, FileInternalMetricsConfig, FileOpen,
        FileSourceInternalEventsEmitter, StreamClosedError,
    },
    line_agg::{self, LineAgg},
    serde::bool_or_struct,
    shutdown::ShutdownSignal,
    SourceSender,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display(
        "message_start_indicator {:?} is not a valid regex: {}",
        indicator,
        source
    ))]
    InvalidMessageStartIndicator {
        indicator: String,
        source: regex::Error,
    },
}

/// Configuration for the `file` source.
#[serde_as]
#[configurable_component(source("file", "Collect logs from files."))]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    /// Array of file patterns to include. [Globbing](https://vector.dev/docs/reference/configuration/sources/file/#globbing) is supported.
    #[configurable(metadata(docs::examples = "/var/log/**/*.log"))]
    pub include: Vec<PathBuf>,

    /// Array of file patterns to exclude. [Globbing](https://vector.dev/docs/reference/configuration/sources/file/#globbing) is supported.
    ///
    /// Takes precedence over the `include` option. Note: The `exclude` patterns are applied _after_ the attempt to glob everything
    /// in `include`. This means that all files are first matched by `include` and then filtered by the `exclude`
    /// patterns. This can be impactful if `include` contains directories with contents that are not accessible.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "/var/log/binary-file.log"))]
    pub exclude: Vec<PathBuf>,

    /// Overrides the name of the log field used to add the file path to each event.
    ///
    /// The value is the full path to the file where the event was read message.
    ///
    /// Set to `""` to suppress this key.
    #[serde(default = "default_file_key")]
    #[configurable(metadata(docs::examples = "path"))]
    pub file_key: OptionalValuePath,

    /// Whether or not to start reading from the beginning of a new file.
    #[configurable(
        deprecated = "This option has been deprecated, use `ignore_checkpoints`/`read_from` instead."
    )]
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub start_at_beginning: Option<bool>,

    /// Whether or not to ignore existing checkpoints when determining where to start reading a file.
    ///
    /// Checkpoints are still written normally.
    #[serde(default)]
    pub ignore_checkpoints: Option<bool>,

    #[serde(default = "default_read_from")]
    #[configurable(derived)]
    pub read_from: ReadFromConfig,

    /// Ignore files with a data modification date older than the specified number of seconds.
    #[serde(alias = "ignore_older", default)]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::examples = 600))]
    #[configurable(metadata(docs::human_name = "Ignore Older Files"))]
    pub ignore_older_secs: Option<u64>,

    /// The maximum size of a line before it is discarded.
    ///
    /// This protects against malformed lines or tailing incorrect files.
    #[serde(default = "default_max_line_bytes")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_line_bytes: usize,

    /// Overrides the name of the log field used to add the current hostname to each event.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// Set to `""` to suppress this key.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    #[configurable(metadata(docs::examples = "hostname"))]
    pub host_key: Option<OptionalValuePath>,

    /// The directory used to persist file checkpoint positions.
    ///
    /// By default, the [global `data_dir` option][global_data_dir] is used.
    /// Make sure the running user has write permissions to this directory.
    ///
    /// If this directory is specified, then Vector will attempt to create it.
    ///
    /// [global_data_dir]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
    #[serde(default)]
    #[configurable(metadata(docs::examples = "/var/local/lib/vector/"))]
    #[configurable(metadata(docs::human_name = "Data Directory"))]
    pub data_dir: Option<PathBuf>,

    /// Enables adding the file offset to each event and sets the name of the log field used.
    ///
    /// The value is the byte offset of the start of the line within the file.
    ///
    /// Off by default, the offset is only added to the event if this is set.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "offset"))]
    pub offset_key: Option<OptionalValuePath>,

    /// The delay between file discovery calls.
    ///
    /// This controls the interval at which files are searched. A higher value results in greater
    /// chances of some short-lived files being missed between searches, but a lower value increases
    /// the performance impact of file discovery.
    #[serde(
        alias = "glob_minimum_cooldown",
        default = "default_glob_minimum_cooldown_ms"
    )]
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::type_unit = "milliseconds"))]
    #[configurable(metadata(docs::human_name = "Glob Minimum Cooldown"))]
    pub glob_minimum_cooldown_ms: Duration,

    #[configurable(derived)]
    #[serde(alias = "fingerprinting", default)]
    fingerprint: FingerprintConfig,

    /// Ignore missing files when fingerprinting.
    ///
    /// This may be useful when used with source directories containing dangling symlinks.
    #[serde(default)]
    pub ignore_not_found: bool,

    /// String value used to identify the start of a multi-line message.
    #[configurable(deprecated = "This option has been deprecated, use `multiline` instead.")]
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub message_start_indicator: Option<String>,

    /// How long to wait for more data when aggregating a multi-line message, in milliseconds.
    #[configurable(deprecated = "This option has been deprecated, use `multiline` instead.")]
    #[configurable(metadata(docs::hidden))]
    #[serde(default = "default_multi_line_timeout")]
    pub multi_line_timeout: u64,

    /// Multiline aggregation configuration.
    ///
    /// If not specified, multiline aggregation is disabled.
    #[configurable(derived)]
    #[serde(default)]
    pub multiline: Option<MultilineConfig>,

    /// Max amount of bytes to read from a single file before switching over to the next file.
    /// **Note:** This does not apply when `oldest_first` is `true`.
    ///
    /// This allows distributing the reads more or less evenly across
    /// the files.
    #[serde(default = "default_max_read_bytes")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_read_bytes: usize,

    /// Instead of balancing read capacity fairly across all watched files, prioritize draining the oldest files before moving on to read data from more recent files.
    #[serde(default)]
    pub oldest_first: bool,

    /// After reaching EOF, the number of seconds to wait before removing the file, unless new data is written.
    ///
    /// If not specified, files are not removed.
    #[serde(alias = "remove_after", default)]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::examples = 0))]
    #[configurable(metadata(docs::examples = 5))]
    #[configurable(metadata(docs::examples = 60))]
    #[configurable(metadata(docs::human_name = "Wait Time Before Removing File"))]
    pub remove_after_secs: Option<u64>,

    /// String sequence used to separate one file line from another.
    #[serde(default = "default_line_delimiter")]
    #[configurable(metadata(docs::examples = "\r\n"))]
    pub line_delimiter: String,

    #[configurable(derived)]
    #[serde(default)]
    pub encoding: Option<EncodingConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default)]
    internal_metrics: FileInternalMetricsConfig,

    /// How long to keep an open handle to a rotated log file.
    /// The default value represents "no limit"
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(default = "default_rotate_wait", rename = "rotate_wait_secs")]
    pub rotate_wait: Duration,
}

fn default_max_line_bytes() -> usize {
    bytesize::kib(100u64) as usize
}

fn default_file_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("file"))
}

const fn default_read_from() -> ReadFromConfig {
    ReadFromConfig::Beginning
}

const fn default_glob_minimum_cooldown_ms() -> Duration {
    Duration::from_millis(1000)
}

const fn default_multi_line_timeout() -> u64 {
    1000
} // deprecated

const fn default_max_read_bytes() -> usize {
    2048
}

fn default_line_delimiter() -> String {
    "\n".to_string()
}

const fn default_rotate_wait() -> Duration {
    Duration::from_secs(u64::MAX / 2)
}

/// Configuration for how files should be identified.
///
/// This is important for `checkpointing` when file rotation is used.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(tag = "strategy", rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "The strategy used to uniquely identify files.\n\nThis is important for checkpointing when file rotation is used."
))]
pub enum FingerprintConfig {
    /// Read lines from the beginning of the file and compute a checksum over them.
    Checksum {
        /// Maximum number of bytes to use, from the lines that are read, for generating the checksum.
        ///
        // TODO: Should we properly expose this in the documentation? There could definitely be value in allowing more
        // bytes to be used for the checksum generation, but we should commit to exposing it rather than hiding it.
        #[serde(alias = "fingerprint_bytes")]
        #[configurable(metadata(docs::hidden))]
        #[configurable(metadata(docs::type_unit = "bytes"))]
        bytes: Option<usize>,

        /// The number of bytes to skip ahead (or ignore) when reading the data used for generating the checksum.
        ///
        /// This can be helpful if all files share a common header that should be skipped.
        #[serde(default = "default_ignored_header_bytes")]
        #[configurable(metadata(docs::type_unit = "bytes"))]
        ignored_header_bytes: usize,

        /// The number of lines to read for generating the checksum.
        ///
        /// If your files share a common header that is not always a fixed size,
        ///
        /// If the file has less than this amount of lines, it won’t be read at all.
        #[serde(default = "default_lines")]
        #[configurable(metadata(docs::type_unit = "lines"))]
        lines: usize,
    },

    /// Use the [device and inode][inode] as the identifier.
    ///
    /// [inode]: https://en.wikipedia.org/wiki/Inode
    #[serde(rename = "device_and_inode")]
    DevInode,
}

impl Default for FingerprintConfig {
    fn default() -> Self {
        Self::Checksum {
            bytes: None,
            ignored_header_bytes: 0,
            lines: default_lines(),
        }
    }
}

const fn default_ignored_header_bytes() -> usize {
    0
}

const fn default_lines() -> usize {
    1
}

impl From<FingerprintConfig> for FingerprintStrategy {
    fn from(config: FingerprintConfig) -> FingerprintStrategy {
        match config {
            FingerprintConfig::Checksum {
                bytes,
                ignored_header_bytes,
                lines,
            } => {
                let bytes = match bytes {
                    Some(bytes) => {
                        warn!(message = "The `fingerprint.bytes` option will be used to convert old file fingerprints created by vector < v0.11.0, but are not supported for new file fingerprints. The first line will be used instead.");
                        bytes
                    }
                    None => 256,
                };
                FingerprintStrategy::Checksum {
                    bytes,
                    ignored_header_bytes,
                    lines,
                }
            }
            FingerprintConfig::DevInode => FingerprintStrategy::DevInode,
        }
    }
}

#[derive(Debug)]
pub(crate) struct FinalizerEntry {
    pub(crate) file_id: FileFingerprint,
    pub(crate) offset: u64,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            include: vec![PathBuf::from("/var/log/**/*.log")],
            exclude: vec![],
            file_key: default_file_key(),
            start_at_beginning: None,
            ignore_checkpoints: None,
            read_from: default_read_from(),
            ignore_older_secs: None,
            max_line_bytes: default_max_line_bytes(),
            fingerprint: FingerprintConfig::default(),
            ignore_not_found: false,
            host_key: None,
            offset_key: None,
            data_dir: None,
            glob_minimum_cooldown_ms: default_glob_minimum_cooldown_ms(),
            message_start_indicator: None,
            multi_line_timeout: default_multi_line_timeout(), // millis
            multiline: None,
            max_read_bytes: default_max_read_bytes(),
            oldest_first: false,
            remove_after_secs: None,
            line_delimiter: default_line_delimiter(),
            encoding: None,
            acknowledgements: Default::default(),
            log_namespace: None,
            internal_metrics: Default::default(),
            rotate_wait: default_rotate_wait(),
        }
    }
}

impl_generate_config_from_default!(FileConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "file")]
impl SourceConfig for FileConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        // add the source name as a subdir, so that multiple sources can
        // operate within the same given data_dir (e.g. the global one)
        // without the file servers' checkpointers interfering with each
        // other
        let data_dir = cx
            .globals
            // source are only global, name can be used for subdir
            .resolve_and_make_data_subdir(self.data_dir.as_ref(), cx.key.id())?;

        // Clippy rule, because async_trait?
        #[allow(clippy::suspicious_else_formatting)]
        {
            if let Some(ref config) = self.multiline {
                let _: line_agg::Config = config.try_into()?;
            }

            if let Some(ref indicator) = self.message_start_indicator {
                Regex::new(indicator)
                    .with_context(|_| InvalidMessageStartIndicatorSnafu { indicator })?;
            }
        }

        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        let log_namespace = cx.log_namespace(self.log_namespace);

        Ok(file_source(
            self,
            data_dir,
            cx.shutdown,
            cx.out,
            acknowledgements,
            log_namespace,
        ))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let file_key = self.file_key.clone().path.map(LegacyKey::Overwrite);
        let host_key = self
            .host_key
            .clone()
            .unwrap_or(log_schema().host_key().cloned().into())
            .path
            .map(LegacyKey::Overwrite);

        let offset_key = self
            .offset_key
            .clone()
            .and_then(|k| k.path)
            .map(LegacyKey::Overwrite);

        let schema_definition = BytesDeserializerConfig
            .schema_definition(global_log_namespace.merge(self.log_namespace))
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                host_key,
                &owned_value_path!("host"),
                Kind::bytes().or_undefined(),
                Some("host"),
            )
            .with_source_metadata(
                Self::NAME,
                offset_key,
                &owned_value_path!("offset"),
                Kind::integer(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                file_key,
                &owned_value_path!("path"),
                Kind::bytes(),
                None,
            );

        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

pub fn file_source(
    config: &FileConfig,
    data_dir: PathBuf,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
    acknowledgements: bool,
    log_namespace: LogNamespace,
) -> super::Source {
    // the include option must be specified but also must contain at least one entry.
    if config.include.is_empty() {
        error!(message = "`include` configuration option must contain at least one file pattern.");
        return Box::pin(future::ready(Err(())));
    }

    let exclude_patterns = config
        .exclude
        .iter()
        .map(|path_buf| path_buf.iter().collect::<std::path::PathBuf>())
        .collect::<Vec<PathBuf>>();
    let ignore_before = calculate_ignore_before(config.ignore_older_secs);
    let glob_minimum_cooldown = config.glob_minimum_cooldown_ms;
    let (ignore_checkpoints, read_from) = reconcile_position_options(
        config.start_at_beginning,
        config.ignore_checkpoints,
        Some(config.read_from),
    );

    let emitter = FileSourceInternalEventsEmitter {
        include_file_metric_tag: config.internal_metrics.include_file_tag,
    };

    let paths_provider = Glob::new(
        &config.include,
        &exclude_patterns,
        MatchOptions::default(),
        emitter.clone(),
    )
    .expect("invalid glob patterns");

    let encoding_charset = config.encoding.clone().map(|e| e.charset);

    // if file encoding is specified, need to convert the line delimiter (present as utf8)
    // to the specified encoding, so that delimiter-based line splitting can work properly
    let line_delimiter_as_bytes = match encoding_charset {
        Some(e) => Encoder::new(e).encode_from_utf8(&config.line_delimiter),
        None => Bytes::from(config.line_delimiter.clone()),
    };

    let checkpointer = Checkpointer::new(&data_dir);
    let file_server = FileServer {
        paths_provider,
        max_read_bytes: config.max_read_bytes,
        ignore_checkpoints,
        read_from,
        ignore_before,
        max_line_bytes: config.max_line_bytes,
        line_delimiter: line_delimiter_as_bytes,
        data_dir,
        glob_minimum_cooldown,
        fingerprinter: Fingerprinter {
            strategy: config.fingerprint.clone().into(),
            max_line_length: config.max_line_bytes,
            ignore_not_found: config.ignore_not_found,
        },
        oldest_first: config.oldest_first,
        remove_after: config.remove_after_secs.map(Duration::from_secs),
        emitter,
        handle: tokio::runtime::Handle::current(),
        rotate_wait: config.rotate_wait,
    };

    let event_metadata = EventMetadata {
        host_key: config
            .host_key
            .clone()
            .unwrap_or(log_schema().host_key().cloned().into())
            .path,
        hostname: crate::get_hostname().ok(),
        file_key: config.file_key.clone().path,
        offset_key: config.offset_key.clone().and_then(|k| k.path),
    };

    let include = config.include.clone();
    let exclude = config.exclude.clone();
    let multiline_config = config.multiline.clone();
    let message_start_indicator = config.message_start_indicator.clone();
    let multi_line_timeout = config.multi_line_timeout;

    let (finalizer, shutdown_checkpointer) = if acknowledgements {
        // The shutdown sent in to the finalizer is the global
        // shutdown handle used to tell it to stop accepting new batch
        // statuses and just wait for the remaining acks to come in.
        let (finalizer, mut ack_stream) = OrderedFinalizer::<FinalizerEntry>::new(None);

        // We set up a separate shutdown signal to tie together the
        // finalizer and the checkpoint writer task in the file
        // server, to make it continue to write out updated
        // checkpoints until all the acks have come in.
        let (send_shutdown, shutdown2) = oneshot::channel::<()>();
        let checkpoints = checkpointer.view();
        tokio::spawn(async move {
            while let Some((status, entry)) = ack_stream.next().await {
                if status == BatchStatus::Delivered {
                    checkpoints.update(entry.file_id, entry.offset);
                }
            }
            send_shutdown.send(())
        });
        (Some(finalizer), shutdown2.map(|_| ()).boxed())
    } else {
        // When not dealing with end-to-end acknowledgements, just
        // clone the global shutdown to stop the checkpoint writer.
        (None, shutdown.clone().map(|_| ()).boxed())
    };

    let checkpoints = checkpointer.view();
    let include_file_metric_tag = config.internal_metrics.include_file_tag;
    Box::pin(async move {
        info!(message = "Starting file server.", include = ?include, exclude = ?exclude);

        let mut encoding_decoder = encoding_charset.map(Decoder::new);

        // sizing here is just a guess
        let (tx, rx) = futures::channel::mpsc::channel::<Vec<Line>>(2);
        let rx = rx
            .map(futures::stream::iter)
            .flatten()
            .map(move |mut line| {
                emit!(FileBytesReceived {
                    byte_size: line.text.len(),
                    file: &line.filename,
                    include_file_metric_tag,
                });
                // transcode each line from the file's encoding charset to utf8
                line.text = match encoding_decoder.as_mut() {
                    Some(d) => d.decode_to_utf8(line.text),
                    None => line.text,
                };
                line
            });

        let messages: Box<dyn Stream<Item = Line> + Send + std::marker::Unpin> =
            if let Some(ref multiline_config) = multiline_config {
                wrap_with_line_agg(
                    rx,
                    multiline_config.try_into().unwrap(), // validated in build
                )
            } else if let Some(msi) = message_start_indicator {
                wrap_with_line_agg(
                    rx,
                    line_agg::Config::for_legacy(
                        Regex::new(&msi).unwrap(), // validated in build
                        multi_line_timeout,
                    ),
                )
            } else {
                Box::new(rx)
            };

        // Once file server ends this will run until it has finished processing remaining
        // logs in the queue.
        let span = Span::current();
        let mut messages = messages.map(move |line| {
            let mut event = create_event(
                line.text,
                line.start_offset,
                &line.filename,
                &event_metadata,
                log_namespace,
                include_file_metric_tag,
            );

            if let Some(finalizer) = &finalizer {
                let (batch, receiver) = BatchNotifier::new_with_receiver();
                event = event.with_batch_notifier(&batch);
                let entry = FinalizerEntry {
                    file_id: line.file_id,
                    offset: line.end_offset,
                };
                finalizer.add(entry, receiver);
            } else {
                checkpoints.update(line.file_id, line.end_offset);
            }
            event
        });
        tokio::spawn(async move {
            match out
                .send_event_stream(&mut messages)
                .instrument(span.or_current())
                .await
            {
                Ok(()) => {
                    debug!("Finished sending.");
                }
                Err(_) => {
                    let (count, _) = messages.size_hint();
                    emit!(StreamClosedError { count });
                }
            }
        });

        let span = info_span!("file_server");
        spawn_blocking(move || {
            let _enter = span.enter();
            let result = file_server.run(tx, shutdown, shutdown_checkpointer, checkpointer);
            emit!(FileOpen { count: 0 });
            // Panic if we encounter any error originating from the file server.
            // We're at the `spawn_blocking` call, the panic will be caught and
            // passed to the `JoinHandle` error, similar to the usual threads.
            result.unwrap();
        })
        .map_err(|error| error!(message="File server unexpectedly stopped.", %error))
        .await
    })
}

/// Emit deprecation warning if the old option is used, and take it into account when determining
/// defaults. Any of the newer options will override it when set directly.
fn reconcile_position_options(
    start_at_beginning: Option<bool>,
    ignore_checkpoints: Option<bool>,
    read_from: Option<ReadFromConfig>,
) -> (bool, ReadFrom) {
    if start_at_beginning.is_some() {
        warn!(message = "Use of deprecated option `start_at_beginning`. Please use `ignore_checkpoints` and `read_from` options instead.")
    }

    match start_at_beginning {
        Some(true) => (
            ignore_checkpoints.unwrap_or(true),
            read_from.map(Into::into).unwrap_or(ReadFrom::Beginning),
        ),
        _ => (
            ignore_checkpoints.unwrap_or(false),
            read_from.map(Into::into).unwrap_or_default(),
        ),
    }
}

fn wrap_with_line_agg(
    rx: impl Stream<Item = Line> + Send + std::marker::Unpin + 'static,
    config: line_agg::Config,
) -> Box<dyn Stream<Item = Line> + Send + std::marker::Unpin + 'static> {
    let logic = line_agg::Logic::new(config);
    Box::new(
        LineAgg::new(
            rx.map(|line| {
                (
                    line.filename,
                    line.text,
                    (line.file_id, line.start_offset, line.end_offset),
                )
            }),
            logic,
        )
        .map(
            |(filename, text, (file_id, start_offset, initial_end), lastline_context)| Line {
                text,
                filename,
                file_id,
                start_offset,
                end_offset: lastline_context.map_or(initial_end, |(_, _, lastline_end_offset)| {
                    lastline_end_offset
                }),
            },
        ),
    )
}

struct EventMetadata {
    host_key: Option<OwnedValuePath>,
    hostname: Option<String>,
    file_key: Option<OwnedValuePath>,
    offset_key: Option<OwnedValuePath>,
}

fn create_event(
    line: Bytes,
    offset: u64,
    file: &str,
    meta: &EventMetadata,
    log_namespace: LogNamespace,
    include_file_metric_tag: bool,
) -> LogEvent {
    let deserializer = BytesDeserializer;
    let mut event = deserializer.parse_single(line, log_namespace);

    log_namespace.insert_vector_metadata(
        &mut event,
        log_schema().source_type_key(),
        path!("source_type"),
        Bytes::from_static(FileConfig::NAME.as_bytes()),
    );
    log_namespace.insert_vector_metadata(
        &mut event,
        log_schema().timestamp_key(),
        path!("ingest_timestamp"),
        Utc::now(),
    );

    let legacy_host_key = meta.host_key.as_ref().map(LegacyKey::Overwrite);
    // `meta.host_key` is already `unwrap_or_else`ed so we can just pass it in.
    if let Some(hostname) = &meta.hostname {
        log_namespace.insert_source_metadata(
            FileConfig::NAME,
            &mut event,
            legacy_host_key,
            path!("host"),
            hostname.clone(),
        );
    }

    let legacy_offset_key = meta.offset_key.as_ref().map(LegacyKey::Overwrite);
    log_namespace.insert_source_metadata(
        FileConfig::NAME,
        &mut event,
        legacy_offset_key,
        path!("offset"),
        offset,
    );

    let legacy_file_key = meta.file_key.as_ref().map(LegacyKey::Overwrite);
    log_namespace.insert_source_metadata(
        FileConfig::NAME,
        &mut event,
        legacy_file_key,
        path!("path"),
        file,
    );

    emit!(FileEventsReceived {
        count: 1,
        file,
        byte_size: event.estimated_json_encoded_size_of(),
        include_file_metric_tag,
    });

    event
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        fs::{self, File},
        future::Future,
        io::{Seek, Write},
    };

    use encoding_rs::UTF_16LE;
    use similar_asserts::assert_eq;
    use tempfile::tempdir;
    use tokio::time::{sleep, timeout, Duration};
    use vector_lib::schema::Definition;
    use vrl::value::kind::Collection;

    use super::*;
    use crate::{
        config::Config,
        event::{Event, EventStatus, Value},
        shutdown::ShutdownSignal,
        sources::file,
        test_util::components::{assert_source_compliance, FILE_SOURCE_TAGS},
    };
    use vrl::value;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<FileConfig>();
    }

    fn test_default_file_config(dir: &tempfile::TempDir) -> file::FileConfig {
        file::FileConfig {
            fingerprint: FingerprintConfig::Checksum {
                bytes: Some(8),
                ignored_header_bytes: 0,
                lines: 1,
            },
            data_dir: Some(dir.path().to_path_buf()),
            glob_minimum_cooldown_ms: Duration::from_millis(100),
            internal_metrics: FileInternalMetricsConfig {
                include_file_tag: true,
            },
            ..Default::default()
        }
    }

    async fn sleep_500_millis() {
        sleep(Duration::from_millis(500)).await;
    }

    #[test]
    fn parse_config() {
        let config: FileConfig = toml::from_str(
            r#"
            include = [ "/var/log/**/*.log" ]
            file_key = "file"
            glob_minimum_cooldown_ms = 1000
            multi_line_timeout = 1000
            max_read_bytes = 2048
            line_delimiter = "\n"
        "#,
        )
        .unwrap();
        assert_eq!(config, FileConfig::default());
        assert_eq!(
            config.fingerprint,
            FingerprintConfig::Checksum {
                bytes: None,
                ignored_header_bytes: 0,
                lines: 1
            }
        );

        let config: FileConfig = toml::from_str(
            r#"
        include = [ "/var/log/**/*.log" ]
        [fingerprint]
        strategy = "device_and_inode"
        "#,
        )
        .unwrap();
        assert_eq!(config.fingerprint, FingerprintConfig::DevInode);

        let config: FileConfig = toml::from_str(
            r#"
        include = [ "/var/log/**/*.log" ]
        [fingerprint]
        strategy = "checksum"
        bytes = 128
        ignored_header_bytes = 512
        "#,
        )
        .unwrap();
        assert_eq!(
            config.fingerprint,
            FingerprintConfig::Checksum {
                bytes: Some(128),
                ignored_header_bytes: 512,
                lines: 1
            }
        );

        let config: FileConfig = toml::from_str(
            r#"
        include = [ "/var/log/**/*.log" ]
        [encoding]
        charset = "utf-16le"
        "#,
        )
        .unwrap();
        assert_eq!(config.encoding, Some(EncodingConfig { charset: UTF_16LE }));

        let config: FileConfig = toml::from_str(
            r#"
        include = [ "/var/log/**/*.log" ]
        read_from = "beginning"
        "#,
        )
        .unwrap();
        assert_eq!(config.read_from, ReadFromConfig::Beginning);

        let config: FileConfig = toml::from_str(
            r#"
        include = [ "/var/log/**/*.log" ]
        read_from = "end"
        "#,
        )
        .unwrap();
        assert_eq!(config.read_from, ReadFromConfig::End);
    }

    #[test]
    fn resolve_data_dir() {
        let global_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();

        let mut config = Config::default();
        config.global.data_dir = global_dir.into_path().into();

        // local path given -- local should win
        let res = config
            .global
            .resolve_and_validate_data_dir(test_default_file_config(&local_dir).data_dir.as_ref())
            .unwrap();
        assert_eq!(res, local_dir.path());

        // no local path given -- global fallback should be in effect
        let res = config.global.resolve_and_validate_data_dir(None).unwrap();
        assert_eq!(res, config.global.data_dir.unwrap());
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let definitions = FileConfig::default()
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        assert_eq!(
            definitions,
            Some(
                Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                    .with_meaning(OwnedTargetPath::event_root(), "message")
                    .with_metadata_field(
                        &owned_value_path!("vector", "source_type"),
                        Kind::bytes(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("vector", "ingest_timestamp"),
                        Kind::timestamp(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("file", "host"),
                        Kind::bytes().or_undefined(),
                        Some("host")
                    )
                    .with_metadata_field(
                        &owned_value_path!("file", "offset"),
                        Kind::integer(),
                        None
                    )
                    .with_metadata_field(&owned_value_path!("file", "path"), Kind::bytes(), None)
            )
        )
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let definitions = FileConfig::default()
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        assert_eq!(
            definitions,
            Some(
                Definition::new_with_default_metadata(
                    Kind::object(Collection::empty()),
                    [LogNamespace::Legacy]
                )
                .with_event_field(
                    &owned_value_path!("message"),
                    Kind::bytes(),
                    Some("message")
                )
                .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
                .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
                .with_event_field(
                    &owned_value_path!("host"),
                    Kind::bytes().or_undefined(),
                    Some("host")
                )
                .with_event_field(&owned_value_path!("offset"), Kind::undefined(), None)
                .with_event_field(&owned_value_path!("file"), Kind::bytes(), None)
            )
        )
    }

    #[test]
    fn create_event_legacy_namespace() {
        let line = Bytes::from("hello world");
        let file = "some_file.rs";
        let offset: u64 = 0;

        let meta = EventMetadata {
            host_key: Some(owned_value_path!("host")),
            hostname: Some("Some.Machine".to_string()),
            file_key: Some(owned_value_path!("file")),
            offset_key: Some(owned_value_path!("offset")),
        };
        let log = create_event(line, offset, file, &meta, LogNamespace::Legacy, false);

        assert_eq!(log["file"], "some_file.rs".into());
        assert_eq!(log["host"], "Some.Machine".into());
        assert_eq!(log["offset"], 0.into());
        assert_eq!(*log.get_message().unwrap(), "hello world".into());
        assert_eq!(*log.get_source_type().unwrap(), "file".into());
        assert!(log[log_schema().timestamp_key().unwrap().to_string()].is_timestamp());
    }

    #[test]
    fn create_event_custom_fields_legacy_namespace() {
        let line = Bytes::from("hello world");
        let file = "some_file.rs";
        let offset: u64 = 0;

        let meta = EventMetadata {
            host_key: Some(owned_value_path!("hostname")),
            hostname: Some("Some.Machine".to_string()),
            file_key: Some(owned_value_path!("file_path")),
            offset_key: Some(owned_value_path!("off")),
        };
        let log = create_event(line, offset, file, &meta, LogNamespace::Legacy, false);

        assert_eq!(log["file_path"], "some_file.rs".into());
        assert_eq!(log["hostname"], "Some.Machine".into());
        assert_eq!(log["off"], 0.into());
        assert_eq!(*log.get_message().unwrap(), "hello world".into());
        assert_eq!(*log.get_source_type().unwrap(), "file".into());
        assert!(log[log_schema().timestamp_key().unwrap().to_string()].is_timestamp());
    }

    #[test]
    fn create_event_vector_namespace() {
        let line = Bytes::from("hello world");
        let file = "some_file.rs";
        let offset: u64 = 0;

        let meta = EventMetadata {
            host_key: Some(owned_value_path!("ignored")),
            hostname: Some("Some.Machine".to_string()),
            file_key: Some(owned_value_path!("ignored")),
            offset_key: Some(owned_value_path!("ignored")),
        };
        let log = create_event(line, offset, file, &meta, LogNamespace::Vector, false);

        assert_eq!(log.value(), &value!("hello world"));

        assert_eq!(
            log.metadata()
                .value()
                .get(path!("vector", "source_type"))
                .unwrap(),
            &value!("file")
        );
        assert!(log
            .metadata()
            .value()
            .get(path!("vector", "ingest_timestamp"))
            .unwrap()
            .is_timestamp());

        assert_eq!(
            log.metadata()
                .value()
                .get(path!(FileConfig::NAME, "host"))
                .unwrap(),
            &value!("Some.Machine")
        );
        assert_eq!(
            log.metadata()
                .value()
                .get(path!(FileConfig::NAME, "offset"))
                .unwrap(),
            &value!(0)
        );
        assert_eq!(
            log.metadata()
                .value()
                .get(path!(FileConfig::NAME, "path"))
                .unwrap(),
            &value!("some_file.rs")
        );
    }

    #[tokio::test]
    async fn file_happy_path() {
        let n = 5;

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path1 = dir.path().join("file1");
        let path2 = dir.path().join("file2");

        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let mut file1 = File::create(&path1).unwrap();
            let mut file2 = File::create(&path2).unwrap();

            sleep_500_millis().await; // The files must be observed at their original lengths before writing to them

            for i in 0..n {
                writeln!(&mut file1, "hello {}", i).unwrap();
                writeln!(&mut file2, "goodbye {}", i).unwrap();
            }

            sleep_500_millis().await;
        })
        .await;

        let mut hello_i = 0;
        let mut goodbye_i = 0;

        for event in received {
            let line =
                event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();
            if line.starts_with("hello") {
                assert_eq!(line, format!("hello {}", hello_i));
                assert_eq!(
                    event.as_log()["file"].to_string_lossy(),
                    path1.to_str().unwrap()
                );
                hello_i += 1;
            } else {
                assert_eq!(line, format!("goodbye {}", goodbye_i));
                assert_eq!(
                    event.as_log()["file"].to_string_lossy(),
                    path2.to_str().unwrap()
                );
                goodbye_i += 1;
            }
        }
        assert_eq!(hello_i, n);
        assert_eq!(goodbye_i, n);
    }

    // https://github.com/vectordotdev/vector/issues/8363
    #[tokio::test]
    async fn file_read_empty_lines() {
        let n = 5;

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");

        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await; // The files must be observed at their original lengths before writing to them

            writeln!(&mut file, "line for checkpointing").unwrap();
            for _i in 0..n {
                writeln!(&mut file).unwrap();
            }

            sleep_500_millis().await;
        })
        .await;

        assert_eq!(received.len(), n + 1);
    }

    #[tokio::test]
    async fn file_truncate() {
        let n = 5;

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };
        let path = dir.path().join("file");
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await; // The files must be observed at its original length before writing to it

            for i in 0..n {
                writeln!(&mut file, "pretrunc {}", i).unwrap();
            }

            sleep_500_millis().await; // The writes must be observed before truncating

            file.set_len(0).unwrap();
            file.seek(std::io::SeekFrom::Start(0)).unwrap();

            sleep_500_millis().await; // The truncate must be observed before writing again

            for i in 0..n {
                writeln!(&mut file, "posttrunc {}", i).unwrap();
            }

            sleep_500_millis().await;
        })
        .await;

        let mut i = 0;
        let mut pre_trunc = true;

        for event in received {
            assert_eq!(
                event.as_log()["file"].to_string_lossy(),
                path.to_str().unwrap()
            );

            let line =
                event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();

            if pre_trunc {
                assert_eq!(line, format!("pretrunc {}", i));
            } else {
                assert_eq!(line, format!("posttrunc {}", i));
            }

            i += 1;
            if i == n {
                i = 0;
                pre_trunc = false;
            }
        }
    }

    #[tokio::test]
    async fn file_rotate() {
        let n = 5;

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let archive_path = dir.path().join("file");
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await; // The files must be observed at its original length before writing to it

            for i in 0..n {
                writeln!(&mut file, "prerot {}", i).unwrap();
            }

            sleep_500_millis().await; // The writes must be observed before rotating

            fs::rename(&path, archive_path).expect("could not rename");
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await; // The rotation must be observed before writing again

            for i in 0..n {
                writeln!(&mut file, "postrot {}", i).unwrap();
            }

            sleep_500_millis().await;
        })
        .await;

        let mut i = 0;
        let mut pre_rot = true;

        for event in received {
            assert_eq!(
                event.as_log()["file"].to_string_lossy(),
                path.to_str().unwrap()
            );

            let line =
                event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();

            if pre_rot {
                assert_eq!(line, format!("prerot {}", i));
            } else {
                assert_eq!(line, format!("postrot {}", i));
            }

            i += 1;
            if i == n {
                i = 0;
                pre_rot = false;
            }
        }
    }

    #[tokio::test]
    async fn file_multiple_paths() {
        let n = 5;

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*.txt"), dir.path().join("a.*")],
            exclude: vec![dir.path().join("a.*.txt")],
            ..test_default_file_config(&dir)
        };

        let path1 = dir.path().join("a.txt");
        let path2 = dir.path().join("b.txt");
        let path3 = dir.path().join("a.log");
        let path4 = dir.path().join("a.ignore.txt");
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let mut file1 = File::create(&path1).unwrap();
            let mut file2 = File::create(&path2).unwrap();
            let mut file3 = File::create(&path3).unwrap();
            let mut file4 = File::create(&path4).unwrap();

            sleep_500_millis().await; // The files must be observed at their original lengths before writing to them

            for i in 0..n {
                writeln!(&mut file1, "1 {}", i).unwrap();
                writeln!(&mut file2, "2 {}", i).unwrap();
                writeln!(&mut file3, "3 {}", i).unwrap();
                writeln!(&mut file4, "4 {}", i).unwrap();
            }

            sleep_500_millis().await;
        })
        .await;

        let mut is = [0; 3];

        for event in received {
            let line =
                event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();
            let mut split = line.split(' ');
            let file = split.next().unwrap().parse::<usize>().unwrap();
            assert_ne!(file, 4);
            let i = split.next().unwrap().parse::<usize>().unwrap();

            assert_eq!(is[file - 1], i);
            is[file - 1] += 1;
        }

        assert_eq!(is, [n as usize; 3]);
    }

    #[tokio::test]
    async fn file_exclude_paths() {
        let n = 5;

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("a//b/*.log.*")],
            exclude: vec![dir.path().join("a//b/test.log.*")],
            ..test_default_file_config(&dir)
        };

        let path1 = dir.path().join("a//b/a.log.1");
        let path2 = dir.path().join("a//b/test.log.1");
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
            let mut file1 = File::create(&path1).unwrap();
            let mut file2 = File::create(&path2).unwrap();

            sleep_500_millis().await; // The files must be observed at their original lengths before writing to them

            for i in 0..n {
                writeln!(&mut file1, "1 {}", i).unwrap();
                writeln!(&mut file2, "2 {}", i).unwrap();
            }

            sleep_500_millis().await;
        })
        .await;

        let mut is = [0; 1];

        for event in received {
            let line =
                event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();
            let mut split = line.split(' ');
            let file = split.next().unwrap().parse::<usize>().unwrap();
            assert_ne!(file, 4);
            let i = split.next().unwrap().parse::<usize>().unwrap();

            assert_eq!(is[file - 1], i);
            is[file - 1] += 1;
        }

        assert_eq!(is, [n as usize; 1]);
    }

    #[tokio::test]
    async fn file_key_acknowledged() {
        file_key(Acks).await
    }

    #[tokio::test]
    async fn file_key_no_acknowledge() {
        file_key(NoAcks).await
    }

    async fn file_key(acks: AckingMode) {
        // Default
        {
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                ..test_default_file_config(&dir)
            };

            let path = dir.path().join("file");
            let received = run_file_source(&config, true, acks, LogNamespace::Legacy, async {
                let mut file = File::create(&path).unwrap();

                sleep_500_millis().await;

                writeln!(&mut file, "hello there").unwrap();

                sleep_500_millis().await;
            })
            .await;

            assert_eq!(received.len(), 1);
            assert_eq!(
                received[0].as_log()["file"].to_string_lossy(),
                path.to_str().unwrap()
            );
        }

        // Custom
        {
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                file_key: OptionalValuePath::from(owned_value_path!("source")),
                ..test_default_file_config(&dir)
            };

            let path = dir.path().join("file");
            let received = run_file_source(&config, true, acks, LogNamespace::Legacy, async {
                let mut file = File::create(&path).unwrap();

                sleep_500_millis().await;

                writeln!(&mut file, "hello there").unwrap();

                sleep_500_millis().await;
            })
            .await;

            assert_eq!(received.len(), 1);
            assert_eq!(
                received[0].as_log()["source"].to_string_lossy(),
                path.to_str().unwrap()
            );
        }

        // Hidden
        {
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                ..test_default_file_config(&dir)
            };

            let path = dir.path().join("file");
            let received = run_file_source(&config, true, acks, LogNamespace::Legacy, async {
                let mut file = File::create(&path).unwrap();

                sleep_500_millis().await;

                writeln!(&mut file, "hello there").unwrap();

                sleep_500_millis().await;
            })
            .await;

            assert_eq!(received.len(), 1);
            assert_eq!(
                received[0].as_log().keys().unwrap().collect::<HashSet<_>>(),
                vec![
                    default_file_key()
                        .path
                        .expect("file key to exist")
                        .to_string()
                        .into(),
                    log_schema().host_key().unwrap().to_string().into(),
                    log_schema().message_key().unwrap().to_string().into(),
                    log_schema().timestamp_key().unwrap().to_string().into(),
                    log_schema().source_type_key().unwrap().to_string().into()
                ]
                .into_iter()
                .collect::<HashSet<_>>()
            );
        }
    }

    #[cfg(target_os = "linux")] // see #7988
    #[tokio::test]
    async fn file_start_position_server_restart_acknowledged() {
        file_start_position_server_restart(Acks).await
    }

    #[cfg(target_os = "linux")] // see #7988
    #[tokio::test]
    async fn file_start_position_server_restart_no_acknowledge() {
        file_start_position_server_restart(NoAcks).await
    }

    #[cfg(target_os = "linux")] // see #7988
    async fn file_start_position_server_restart(acking: AckingMode) {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();
        writeln!(&mut file, "zeroth line").unwrap();
        sleep_500_millis().await;

        // First time server runs it picks up existing lines.
        {
            let received = run_file_source(&config, true, acking, LogNamespace::Legacy, async {
                sleep_500_millis().await;
                writeln!(&mut file, "first line").unwrap();
                sleep_500_millis().await;
            })
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["zeroth line", "first line"]);
        }
        // Restart server, read file from checkpoint.
        {
            let received = run_file_source(&config, true, acking, LogNamespace::Legacy, async {
                sleep_500_millis().await;
                writeln!(&mut file, "second line").unwrap();
                sleep_500_millis().await;
            })
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["second line"]);
        }
        // Restart server, read files from beginning.
        {
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                ignore_checkpoints: Some(true),
                read_from: ReadFromConfig::Beginning,
                ..test_default_file_config(&dir)
            };
            let received = run_file_source(&config, false, acking, LogNamespace::Legacy, async {
                sleep_500_millis().await;
                writeln!(&mut file, "third line").unwrap();
                sleep_500_millis().await;
            })
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(
                lines,
                vec!["zeroth line", "first line", "second line", "third line"]
            );
        }
    }

    #[tokio::test]
    async fn file_start_position_server_restart_unfinalized() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();
        writeln!(&mut file, "the line").unwrap();
        sleep_500_millis().await;

        // First time server runs it picks up existing lines.
        let received = run_file_source(
            &config,
            false,
            Unfinalized,
            LogNamespace::Legacy,
            sleep_500_millis(),
        )
        .await;
        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["the line"]);

        // Restart server, it re-reads file since the events were not acknowledged before shutdown
        let received = run_file_source(
            &config,
            false,
            Unfinalized,
            LogNamespace::Legacy,
            sleep_500_millis(),
        )
        .await;
        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["the line"]);
    }

    #[tokio::test]
    async fn file_duplicate_processing_after_restart() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        let line_count = 4000;
        for i in 0..line_count {
            writeln!(&mut file, "Here's a line for you: {}", i).unwrap();
        }
        sleep_500_millis().await;

        // First time server runs it should pick up a bunch of lines
        let received = run_file_source(
            &config,
            true,
            Acks,
            LogNamespace::Legacy,
            // shutdown signal is sent after this duration
            sleep_500_millis(),
        )
        .await;
        let lines = extract_messages_string(received);

        // ...but not all the lines; if the first run processed the entire file, we may not hit the
        // bug we're testing for, which happens if the finalizer stream exits on shutdown with pending acks
        assert!(lines.len() < line_count);

        // Restart the server, and it should read the rest without duplicating any
        let received = run_file_source(
            &config,
            true,
            Acks,
            LogNamespace::Legacy,
            sleep(Duration::from_secs(5)),
        )
        .await;
        let lines2 = extract_messages_string(received);

        // Between both runs, we should have the expected number of lines
        assert_eq!(lines.len() + lines2.len(), line_count);
    }

    #[tokio::test]
    async fn file_start_position_server_restart_with_file_rotation_acknowledged() {
        file_start_position_server_restart_with_file_rotation(Acks).await
    }

    #[tokio::test]
    async fn file_start_position_server_restart_with_file_rotation_no_acknowledge() {
        file_start_position_server_restart_with_file_rotation(NoAcks).await
    }

    async fn file_start_position_server_restart_with_file_rotation(acking: AckingMode) {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let path_for_old_file = dir.path().join("file.old");
        // Run server first time, collect some lines.
        {
            let received = run_file_source(&config, true, acking, LogNamespace::Legacy, async {
                let mut file = File::create(&path).unwrap();
                sleep_500_millis().await;
                writeln!(&mut file, "first line").unwrap();
                sleep_500_millis().await;
            })
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["first line"]);
        }
        // Perform 'file rotation' to archive old lines.
        fs::rename(&path, &path_for_old_file).expect("could not rename");
        // Restart the server and make sure it does not re-read the old file
        // even though it has a new name.
        {
            let received = run_file_source(&config, false, acking, LogNamespace::Legacy, async {
                let mut file = File::create(&path).unwrap();
                sleep_500_millis().await;
                writeln!(&mut file, "second line").unwrap();
                sleep_500_millis().await;
            })
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["second line"]);
        }
    }

    #[cfg(unix)] // this test uses unix-specific function `futimes` during test time
    #[tokio::test]
    async fn file_start_position_ignore_old_files() {
        use std::{
            os::unix::io::AsRawFd,
            time::{Duration, SystemTime},
        };

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ignore_older_secs: Some(5),
            ..test_default_file_config(&dir)
        };

        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let before_path = dir.path().join("before");
            let mut before_file = File::create(&before_path).unwrap();
            let after_path = dir.path().join("after");
            let mut after_file = File::create(&after_path).unwrap();

            writeln!(&mut before_file, "first line").unwrap(); // first few bytes make up unique file fingerprint
            writeln!(&mut after_file, "_first line").unwrap(); //   and therefore need to be non-identical

            {
                // Set the modified times
                let before = SystemTime::now() - Duration::from_secs(8);
                let after = SystemTime::now() - Duration::from_secs(2);

                let before_time = libc::timeval {
                    tv_sec: before
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as _,
                    tv_usec: 0,
                };
                let before_times = [before_time, before_time];

                let after_time = libc::timeval {
                    tv_sec: after
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as _,
                    tv_usec: 0,
                };
                let after_times = [after_time, after_time];

                unsafe {
                    libc::futimes(before_file.as_raw_fd(), before_times.as_ptr());
                    libc::futimes(after_file.as_raw_fd(), after_times.as_ptr());
                }
            }

            sleep_500_millis().await;
            writeln!(&mut before_file, "second line").unwrap();
            writeln!(&mut after_file, "_second line").unwrap();

            sleep_500_millis().await;
        })
        .await;

        let before_lines = received
            .iter()
            .filter(|event| event.as_log()["file"].to_string_lossy().ends_with("before"))
            .map(|event| {
                event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy()
            })
            .collect::<Vec<_>>();
        let after_lines = received
            .iter()
            .filter(|event| event.as_log()["file"].to_string_lossy().ends_with("after"))
            .map(|event| {
                event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy()
            })
            .collect::<Vec<_>>();
        assert_eq!(before_lines, vec!["second line"]);
        assert_eq!(after_lines, vec!["_first line", "_second line"]);
    }

    #[tokio::test]
    async fn file_max_line_bytes() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            max_line_bytes: 10,
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await; // The files must be observed at their original lengths before writing to them

            writeln!(&mut file, "short").unwrap();
            writeln!(&mut file, "this is too long").unwrap();
            writeln!(&mut file, "11 eleven11").unwrap();
            let super_long = "This line is super long and will take up more space than BufReader's internal buffer, just to make sure that everything works properly when multiple read calls are involved".repeat(10000);
            writeln!(&mut file, "{}", super_long).unwrap();
            writeln!(&mut file, "exactly 10").unwrap();
            writeln!(&mut file, "it can end on a line that's too long").unwrap();

            sleep_500_millis().await;
            sleep_500_millis().await;

            writeln!(&mut file, "and then continue").unwrap();
            writeln!(&mut file, "last short").unwrap();

            sleep_500_millis().await;
            sleep_500_millis().await;
        }).await;

        let received = extract_messages_value(received);

        assert_eq!(
            received,
            vec!["short".into(), "exactly 10".into(), "last short".into()]
        );
    }

    #[tokio::test]
    async fn test_multi_line_aggregation_legacy() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            message_start_indicator: Some("INFO".into()),
            multi_line_timeout: 25, // less than 50 in sleep()
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await; // The files must be observed at their original lengths before writing to them

            writeln!(&mut file, "leftover foo").unwrap();
            writeln!(&mut file, "INFO hello").unwrap();
            writeln!(&mut file, "INFO goodbye").unwrap();
            writeln!(&mut file, "part of goodbye").unwrap();

            sleep_500_millis().await;

            writeln!(&mut file, "INFO hi again").unwrap();
            writeln!(&mut file, "and some more").unwrap();
            writeln!(&mut file, "INFO hello").unwrap();

            sleep_500_millis().await;

            writeln!(&mut file, "too slow").unwrap();
            writeln!(&mut file, "INFO doesn't have").unwrap();
            writeln!(&mut file, "to be INFO in").unwrap();
            writeln!(&mut file, "the middle").unwrap();

            sleep_500_millis().await;
        })
        .await;

        let received = extract_messages_value(received);

        assert_eq!(
            received,
            vec![
                "leftover foo".into(),
                "INFO hello".into(),
                "INFO goodbye\npart of goodbye".into(),
                "INFO hi again\nand some more".into(),
                "INFO hello".into(),
                "too slow".into(),
                "INFO doesn't have".into(),
                "to be INFO in\nthe middle".into(),
            ]
        );
    }

    #[tokio::test]
    async fn test_multi_line_aggregation() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            multiline: Some(MultilineConfig {
                start_pattern: "INFO".to_owned(),
                condition_pattern: "INFO".to_owned(),
                mode: line_agg::Mode::HaltBefore,
                timeout_ms: Duration::from_millis(25), // less than 50 in sleep()
            }),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await; // The files must be observed at their original lengths before writing to them

            writeln!(&mut file, "leftover foo").unwrap();
            writeln!(&mut file, "INFO hello").unwrap();
            writeln!(&mut file, "INFO goodbye").unwrap();
            writeln!(&mut file, "part of goodbye").unwrap();

            sleep_500_millis().await;

            writeln!(&mut file, "INFO hi again").unwrap();
            writeln!(&mut file, "and some more").unwrap();
            writeln!(&mut file, "INFO hello").unwrap();

            sleep_500_millis().await;

            writeln!(&mut file, "too slow").unwrap();
            writeln!(&mut file, "INFO doesn't have").unwrap();
            writeln!(&mut file, "to be INFO in").unwrap();
            writeln!(&mut file, "the middle").unwrap();

            sleep_500_millis().await;
        })
        .await;

        let received = extract_messages_value(received);

        assert_eq!(
            received,
            vec![
                "leftover foo".into(),
                "INFO hello".into(),
                "INFO goodbye\npart of goodbye".into(),
                "INFO hi again\nand some more".into(),
                "INFO hello".into(),
                "too slow".into(),
                "INFO doesn't have".into(),
                "to be INFO in\nthe middle".into(),
            ]
        );
    }

    #[tokio::test]
    async fn test_multi_line_checkpointing() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            offset_key: Some(OptionalValuePath::from(owned_value_path!("offset"))),
            multiline: Some(MultilineConfig {
                start_pattern: "INFO".to_owned(),
                condition_pattern: "INFO".to_owned(),
                mode: line_agg::Mode::HaltBefore,
                timeout_ms: Duration::from_millis(25), // less than 50 in sleep()
            }),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        writeln!(&mut file, "INFO hello").unwrap();
        writeln!(&mut file, "part of hello").unwrap();

        // Read and aggregate existing lines
        let received = run_file_source(
            &config,
            false,
            Acks,
            LogNamespace::Legacy,
            sleep_500_millis(),
        )
        .await;

        assert_eq!(received[0].as_log()["offset"], 0.into());

        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["INFO hello\npart of hello"]);

        // After restart, we should not see any part of the previously aggregated lines
        let received_after_restart =
            run_file_source(&config, false, Acks, LogNamespace::Legacy, async {
                writeln!(&mut file, "INFO goodbye").unwrap();
            })
            .await;
        assert_eq!(
            received_after_restart[0].as_log()["offset"],
            (lines[0].len() + 1).into()
        );
        let lines = extract_messages_string(received_after_restart);
        assert_eq!(lines, vec!["INFO goodbye"]);
    }

    #[tokio::test]
    async fn test_fair_reads() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            max_read_bytes: 1,
            oldest_first: false,
            ..test_default_file_config(&dir)
        };

        let older_path = dir.path().join("z_older_file");
        let mut older = File::create(&older_path).unwrap();

        sleep_500_millis().await;

        let newer_path = dir.path().join("a_newer_file");
        let mut newer = File::create(&newer_path).unwrap();

        writeln!(&mut older, "hello i am the old file").unwrap();
        writeln!(&mut older, "i have been around a while").unwrap();
        writeln!(&mut older, "you can read newer files at the same time").unwrap();

        writeln!(&mut newer, "and i am the new file").unwrap();
        writeln!(&mut newer, "this should be interleaved with the old one").unwrap();
        writeln!(&mut newer, "which is fine because we want fairness").unwrap();

        sleep_500_millis().await;

        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            sleep_500_millis(),
        )
        .await;

        let received = extract_messages_value(received);

        assert_eq!(
            received,
            vec![
                "hello i am the old file".into(),
                "and i am the new file".into(),
                "i have been around a while".into(),
                "this should be interleaved with the old one".into(),
                "you can read newer files at the same time".into(),
                "which is fine because we want fairness".into(),
            ]
        );
    }

    #[tokio::test]
    async fn test_oldest_first() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            max_read_bytes: 1,
            oldest_first: true,
            ..test_default_file_config(&dir)
        };

        let older_path = dir.path().join("z_older_file");
        let mut older = File::create(&older_path).unwrap();

        sleep_500_millis().await;

        let newer_path = dir.path().join("a_newer_file");
        let mut newer = File::create(&newer_path).unwrap();

        writeln!(&mut older, "hello i am the old file").unwrap();
        writeln!(&mut older, "i have been around a while").unwrap();
        writeln!(&mut older, "you should definitely read all of me first").unwrap();

        writeln!(&mut newer, "i'm new").unwrap();
        writeln!(&mut newer, "hopefully you read all the old stuff first").unwrap();
        writeln!(&mut newer, "because otherwise i'm not going to make sense").unwrap();

        sleep_500_millis().await;

        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            sleep_500_millis(),
        )
        .await;

        let received = extract_messages_value(received);

        assert_eq!(
            received,
            vec![
                "hello i am the old file".into(),
                "i have been around a while".into(),
                "you should definitely read all of me first".into(),
                "i'm new".into(),
                "hopefully you read all the old stuff first".into(),
                "because otherwise i'm not going to make sense".into(),
            ]
        );
    }

    // Ignoring on mac: https://github.com/vectordotdev/vector/issues/8373
    #[cfg(not(target_os = "macos"))]
    #[tokio::test]
    async fn test_split_reads() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            max_read_bytes: 1,
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        writeln!(&mut file, "hello i am a normal line").unwrap();

        sleep_500_millis().await;

        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            sleep_500_millis().await;

            write!(&mut file, "i am not a full line").unwrap();

            // Longer than the EOF timeout
            sleep_500_millis().await;

            writeln!(&mut file, " until now").unwrap();

            sleep_500_millis().await;
        })
        .await;

        let received = extract_messages_value(received);

        assert_eq!(
            received,
            vec![
                "hello i am a normal line".into(),
                "i am not a full line until now".into(),
            ]
        );
    }

    #[tokio::test]
    async fn test_gzipped_file() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![PathBuf::from("tests/data/gzipped.log")],
            // TODO: remove this once files are fingerprinted after decompression
            //
            // Currently, this needs to be smaller than the total size of the compressed file
            // because the fingerprinter tries to read until a newline, which it's not going to see
            // in the compressed data, or this number of bytes. If it hits EOF before that, it
            // can't return a fingerprint because the value would change once more data is written.
            max_line_bytes: 100,
            ..test_default_file_config(&dir)
        };

        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            sleep_500_millis(),
        )
        .await;

        let received = extract_messages_value(received);

        assert_eq!(
            received,
            vec![
                "this is a simple file".into(),
                "i have been compressed".into(),
                "in order to make me smaller".into(),
                "but you can still read me".into(),
                "hooray".into(),
            ]
        );
    }

    #[tokio::test]
    async fn test_non_utf8_encoded_file() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![PathBuf::from("tests/data/utf-16le.log")],
            encoding: Some(EncodingConfig { charset: UTF_16LE }),
            ..test_default_file_config(&dir)
        };

        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            sleep_500_millis(),
        )
        .await;

        let received = extract_messages_value(received);

        assert_eq!(
            received,
            vec![
                "hello i am a file".into(),
                "i can unicode".into(),
                "but i do so in 16 bits".into(),
                "and when i byte".into(),
                "i become little-endian".into(),
            ]
        );
    }

    #[tokio::test]
    async fn test_non_default_line_delimiter() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            line_delimiter: "\r\n".to_string(),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, async {
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await; // The files must be observed at their original lengths before writing to them

            write!(&mut file, "hello i am a line\r\n").unwrap();
            write!(&mut file, "and i am too\r\n").unwrap();
            write!(&mut file, "CRLF is how we end\r\n").unwrap();
            write!(&mut file, "please treat us well\r\n").unwrap();

            sleep_500_millis().await;
        })
        .await;

        let received = extract_messages_value(received);

        assert_eq!(
            received,
            vec![
                "hello i am a line".into(),
                "and i am too".into(),
                "CRLF is how we end".into(),
                "please treat us well".into()
            ]
        );
    }

    #[tokio::test]
    async fn remove_file() {
        let n = 5;
        let remove_after_secs = 1;

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            remove_after_secs: Some(remove_after_secs),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let received = run_file_source(&config, false, Acks, LogNamespace::Legacy, async {
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await; // The files must be observed at their original lengths before writing to them

            for i in 0..n {
                writeln!(&mut file, "{}", i).unwrap();
            }
            drop(file);

            for _ in 0..10 {
                // Wait for remove grace period to end.
                sleep(Duration::from_secs(remove_after_secs + 1)).await;

                if File::open(&path).is_err() {
                    break;
                }
            }
        })
        .await;

        assert_eq!(received.len(), n);

        match File::open(&path) {
            Ok(_) => panic!("File wasn't removed"),
            Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::NotFound),
        }
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum AckingMode {
        NoAcks,      // No acknowledgement handling and no finalization
        Unfinalized, // Acknowledgement handling but no finalization
        Acks,        // Full acknowledgements and proper finalization
    }
    use vector_lib::lookup::OwnedTargetPath;
    use AckingMode::*;

    async fn run_file_source(
        config: &FileConfig,
        wait_shutdown: bool,
        acking_mode: AckingMode,
        log_namespace: LogNamespace,
        inner: impl Future<Output = ()>,
    ) -> Vec<Event> {
        assert_source_compliance(&FILE_SOURCE_TAGS, async move {
            let (tx, rx) = if acking_mode == Acks {
                let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);
                (tx, rx.boxed())
            } else {
                let (tx, rx) = SourceSender::new_test();
                (tx, rx.boxed())
            };

            let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();
            let data_dir = config.data_dir.clone().unwrap();
            let acks = !matches!(acking_mode, NoAcks);

            tokio::spawn(file::file_source(
                config,
                data_dir,
                shutdown,
                tx,
                acks,
                log_namespace,
            ));

            inner.await;

            drop(trigger_shutdown);

            let result = if acking_mode == Unfinalized {
                rx.take_until(tokio::time::sleep(Duration::from_secs(5)))
                    .collect::<Vec<_>>()
                    .await
            } else {
                timeout(Duration::from_secs(5), rx.collect::<Vec<_>>())
                    .await
                    .expect(
                        "Unclosed channel: may indicate file-server could not shutdown gracefully.",
                    )
            };
            if wait_shutdown {
                shutdown_done.await;
            }

            result
        })
        .await
    }

    fn extract_messages_string(received: Vec<Event>) -> Vec<String> {
        received
            .into_iter()
            .map(Event::into_log)
            .map(|log| log.get_message().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    fn extract_messages_value(received: Vec<Event>) -> Vec<Value> {
        received
            .into_iter()
            .map(Event::into_log)
            .map(|log| log.get_message().unwrap().clone())
            .collect()
    }
}
