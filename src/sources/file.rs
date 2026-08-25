use std::{convert::TryInto, future, path::PathBuf, time::Duration};

use bytes::Bytes;
use chrono::Utc;
use futures::{FutureExt, Stream, StreamExt, TryFutureExt};
use regex::bytes::Regex;
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use tokio::sync::oneshot;
use tracing::{Instrument, Span};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{BytesDeserializer, BytesDeserializerConfig},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    file_source::{
        file_server::{FileServer, Line, calculate_ignore_before},
        paths_provider::{Glob, MatchOptions},
    },
    file_source_common::{
        Checkpointer, FileFingerprint, FingerprintStrategy, Fingerprinter, ReadFrom, ReadFromConfig,
    },
    finalizer::OrderedFinalizer,
    lookup::{OwnedValuePath, lookup_v2::OptionalValuePath, owned_value_path, path},
};
use vrl::value::Kind;

use super::util::{EncodingConfig, MultilineConfig};
use crate::{
    SourceSender,
    config::{
        DataType, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
        log_schema,
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
        /// The number of bytes to skip ahead (or ignore) when reading the data used for generating the checksum.
        /// If the file is compressed, the number of bytes refer to the header in the uncompressed content. Only
        /// gzip is supported at this time.
        ///
        /// This can be helpful if all files share a common header that should be skipped.
        #[serde(default = "default_ignored_header_bytes")]
        #[configurable(metadata(docs::type_unit = "bytes"))]
        ignored_header_bytes: usize,

        /// The number of lines to read for generating the checksum.
        ///
        /// The number of lines are determined from the uncompressed content if the file is compressed. Only
        /// gzip is supported at this time.
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
                ignored_header_bytes,
                lines,
            } => FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes,
                lines,
            },
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
        error!(
            message = "`include` configuration option must contain at least one file pattern.",
            internal_log_rate_limit = false
        );
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
    let strategy = config.fingerprint.clone().into();

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
        fingerprinter: Fingerprinter::new(strategy, config.max_line_bytes, config.ignore_not_found),
        oldest_first: config.oldest_first,
        remove_after: config.remove_after_secs.map(Duration::from_secs),
        emitter,
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
        crate::spawn_in_current_span(async move {
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
                // checkpoints.update will be called from ack_stream's thread
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
        tokio::task::spawn_blocking(move || {
            let _enter = span.enter();
            let rt = tokio::runtime::Handle::current();
            let result =
                rt.block_on(file_server.run(tx, shutdown, shutdown_checkpointer, checkpointer));
            emit!(FileOpen { count: 0 });
            // Panic if we encounter any error originating from the file server.
            // We're at the `spawn_blocking` call, the panic will be caught and
            // passed to the `JoinHandle` error, similar to the usual threads.
            result.expect("file server exited with an error");
        })
        .map_err(|error| error!(message="File server unexpectedly stopped.", %error, internal_log_rate_limit = false))
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
        warn!(
            message = "Use of deprecated option `start_at_beginning`. Please use `ignore_checkpoints` and `read_from` options instead."
        )
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
mod tests;
