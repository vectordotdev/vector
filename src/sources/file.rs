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
        file_server::{FileDiscoveryMode, FileServer, Line, calculate_ignore_before},
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
        FileSourceInternalEventsEmitter, FilesIdle, StreamClosedError,
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

    #[serde(default)]
    pub encoding: Option<EncodingConfig>,

    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[serde(default)]
    internal_metrics: FileInternalMetricsConfig,

    /// How long to keep an open handle to a rotated log file.
    /// The default value represents "no limit"
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(default = "default_rotate_wait", rename = "rotate_wait_secs")]
    pub rotate_wait: Duration,

    /// The mechanism used to discover new files, detect renames, and wake up reads of existing
    /// files.
    ///
    /// `polling` (the default) re-scans the `include` glob patterns on a fixed interval
    /// (`glob_minimum_cooldown_ms`) and keeps an open file handle for every matched file for as
    /// long as it exists on disk, even files excluded from reading by `ignore_older`. This is
    /// simple and works identically everywhere, but can be expensive when a very large number of
    /// files match `include`.
    ///
    /// `notify` uses OS-level file system event notifications (inotify on Linux, FSEvents on
    /// macOS, `ReadDirectoryChangesW` on Windows) to discover files and wake up reads promptly,
    /// without needing to re-scan or hold a handle open for inactive files. A much less frequent
    /// periodic reconciliation pass (`reconcile_interval_secs`) still runs as a correctness
    /// backstop, since OS-level notification queues can silently overflow. This mode is newer
    /// and has had less production exposure than `polling`.
    #[serde(default)]
    pub file_discovery_mode: FileDiscoveryModeConfig,

    /// How often to run the full glob+fingerprint reconciliation pass when
    /// `file_discovery_mode` is `notify`. This exists purely as a correctness backstop for
    /// OS-level file watch events that were dropped (e.g. due to queue overflow) or that
    /// occurred before the watch was established. Ignored when `file_discovery_mode` is
    /// `polling`.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(
        default = "default_reconcile_interval_secs",
        rename = "reconcile_interval_secs"
    )]
    pub reconcile_interval: Duration,

    /// How long to wait, after a file has been fully read (reached EOF) and stops receiving new
    /// data, before closing its file handle.
    ///
    /// Vector keeps polling the file's metadata (size and modification time) cheaply, without
    /// holding the handle open, and transparently reopens the file if new data arrives. This
    /// avoids holding a large number of open file handles for files that are being watched (for
    /// example, due to `ignore_older_secs` not yet excluding them, or simply because they haven't
    /// rotated out of the `include` glob yet) but are not actively being written to. Applies
    /// regardless of `file_discovery_mode`: `notify` makes *discovering* files fast, but doesn't
    /// by itself stop an already-discovered file from holding a handle open indefinitely -- this
    /// option is what does that.
    ///
    /// After the handle is closed, rotation recovery can identify the old file at a path reported
    /// by `notify` or below its previous parent directory. If a rotator moves it outside both of
    /// those areas, there is no portable way to find the file after its handle is closed. Set this
    /// option to `null` when arbitrary cross-directory rotation must be supported.
    ///
    /// This also applies at startup: a file that also matches `ignore_older_secs` is only opened
    /// briefly to check whether it is gzip-compressed and to capture its identity, then the handle
    /// is closed when Vector can determine that there is no new data to read (either because its
    /// on-disk size already matches its stored checkpoint position, or because it isn't
    /// gzip-compressed, in which case an old file is never read from regardless of checkpoint).
    ///
    /// Defaults to 60 seconds. Set this explicitly to `null` to disable idle-timeout-based closing
    /// entirely, so that active file handles are only ever closed by other means (for example,
    /// rotation via `rotate_wait_secs`), matching Vector's behavior prior to this option's
    /// introduction.
    #[serde(default = "default_idle_timeout_secs")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::examples = 60))]
    #[configurable(metadata(docs::human_name = "Idle Timeout"))]
    pub idle_timeout_secs: Option<u64>,
}

/// The mechanism `file` uses to discover new files, detect renames, and wake up reads of
/// existing files.
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileDiscoveryModeConfig {
    /// Re-scan the `include` glob patterns on a fixed interval (`glob_minimum_cooldown_ms`).
    #[default]
    Polling,
    /// Use OS-level file system event notifications to discover files and wake up reads
    /// promptly, falling back to a periodic reconciliation pass (`reconcile_interval_secs`) as a
    /// correctness backstop.
    Notify,
}

impl From<FileDiscoveryModeConfig> for FileDiscoveryMode {
    fn from(config: FileDiscoveryModeConfig) -> FileDiscoveryMode {
        match config {
            FileDiscoveryModeConfig::Polling => FileDiscoveryMode::PollingOnly,
            FileDiscoveryModeConfig::Notify => FileDiscoveryMode::Notify,
        }
    }
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

/// Justification: this is meant to be a correctness backstop, not the primary discovery
/// mechanism, when `file_discovery_mode = notify`. It only needs to be frequent enough to
/// recover promptly from a dropped/overflowed OS event queue or a missed pre-watch change,
/// not frequent enough to serve as the main polling loop the way `glob_minimum_cooldown_ms`
/// did. Five minutes bounds the worst-case "silently missed a file" window to something
/// operators can reason about, while keeping the reconciliation pass (a full glob + fingerprint
/// scan over every matched file) rare enough that it doesn't reintroduce the cost this mode
/// exists to avoid.
const fn default_reconcile_interval_secs() -> Duration {
    Duration::from_secs(300)
}

/// Default `idle_timeout_secs`: 60 seconds of no new data after reaching EOF before a file's
/// handle is closed. This is deliberately much longer than the read backoff (which tops out at
/// 250ms) so that ordinary, bursty log writers don't cause handles to be repeatedly closed and
/// reopened; it is deliberately not "no limit" (unlike `rotate_wait`) because the entire point of
/// this option is to bound the number of concurrently open handles by default.
const fn default_idle_timeout_secs() -> Option<u64> {
    Some(60)
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
            file_discovery_mode: FileDiscoveryModeConfig::default(),
            reconcile_interval: default_reconcile_interval_secs(),
            idle_timeout_secs: default_idle_timeout_secs(),
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
        discovery_mode: FileDiscoveryMode::from(config.file_discovery_mode),
        reconcile_interval: config.reconcile_interval,
        idle_timeout: config.idle_timeout_secs.map(Duration::from_secs),
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
            emit!(FilesIdle { count: 0 });
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
mod tests {
    use std::{
        collections::HashSet,
        fs::{self, File},
        future::Future,
        io::{Seek, Write},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use encoding_rs::UTF_16LE;
    use indoc::indoc;
    use similar_asserts::assert_eq;
    use tempfile::tempdir;
    use tokio::time::{Duration, sleep, timeout};
    use vector_lib::schema::Definition;
    use vrl::{value, value::kind::Collection};

    use super::*;
    use crate::{
        config::Config,
        event::{Event, EventStatus, Value},
        shutdown::ShutdownSignal,
        sources::file,
        test_util::{
            components::{FILE_SOURCE_TAGS, assert_source_compliance},
            wait_for_atomic_usize_timeout_ms,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<FileConfig>();
    }

    fn test_default_file_config(dir: &tempfile::TempDir) -> file::FileConfig {
        // Store checkpoints in a subdirectory so they don't appear in the
        // glob-watched directory (which covers dir.path()/*).
        let data_dir = dir.path().join(".data");
        fs::create_dir_all(&data_dir).unwrap();
        file::FileConfig {
            fingerprint: FingerprintConfig::Checksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            data_dir: Some(data_dir),
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
        let config: FileConfig = serde_yaml::from_str(indoc! {
            r#"
            include:
              - /var/log/**/*.log
            file_key: file
            glob_minimum_cooldown_ms: 1000
            multi_line_timeout: 1000
            max_read_bytes: 2048
            line_delimiter: "\n"
            "#,
        })
        .unwrap();
        assert_eq!(config, FileConfig::default());
        assert_eq!(
            config.fingerprint,
            FingerprintConfig::Checksum {
                ignored_header_bytes: 0,
                lines: 1
            }
        );

        let config: FileConfig = serde_yaml::from_str(indoc! {
            r#"
            include:
              - /var/log/**/*.log
            fingerprint:
              strategy: device_and_inode
            "#,
        })
        .unwrap();
        assert_eq!(config.fingerprint, FingerprintConfig::DevInode);

        let config: FileConfig = serde_yaml::from_str(indoc! {
            r#"
            include:
              - /var/log/**/*.log
            fingerprint:
              strategy: checksum
              bytes: 128
              ignored_header_bytes: 512
            "#,
        })
        .unwrap();
        assert_eq!(
            config.fingerprint,
            FingerprintConfig::Checksum {
                ignored_header_bytes: 512,
                lines: 1
            }
        );

        let config: FileConfig = serde_yaml::from_str(indoc! {
            r#"
            include:
              - /var/log/**/*.log
            encoding:
              charset: utf-16le
            "#,
        })
        .unwrap();
        assert_eq!(config.encoding, Some(EncodingConfig { charset: UTF_16LE }));

        let config: FileConfig = serde_yaml::from_str(indoc! {
            r#"
            include:
              - /var/log/**/*.log
            read_from: beginning
            "#,
        })
        .unwrap();
        assert_eq!(config.read_from, ReadFromConfig::Beginning);

        let config: FileConfig = serde_yaml::from_str(indoc! {
            r#"
            include:
              - /var/log/**/*.log
            read_from: end
            "#,
        })
        .unwrap();
        assert_eq!(config.read_from, ReadFromConfig::End);
    }

    #[test]
    fn resolve_data_dir() {
        let global_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();

        let mut config = Config::default();
        config.global.data_dir = global_dir.keep().into();

        // local path given -- local should win
        let local_data_dir = Some(local_dir.path().to_path_buf());
        let res = config
            .global
            .resolve_and_validate_data_dir(local_data_dir.as_ref())
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
        assert!(
            log.metadata()
                .value()
                .get(path!("vector", "ingest_timestamp"))
                .unwrap()
                .is_timestamp()
        );

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

        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            let mut file1 = File::create(&path1).unwrap();
            let mut file2 = File::create(&path2).unwrap();

            for i in 0..n {
                writeln!(&mut file1, "hello {i}").unwrap();
                writeln!(&mut file2, "goodbye {i}").unwrap();
            }

            file1.flush().unwrap();
            file2.flush().unwrap();

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

        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            writeln!(&mut file, "line for checkpointing").unwrap();
            for _i in 0..n {
                writeln!(&mut file).unwrap();
            }
            file.flush().unwrap();

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
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            for i in 0..n {
                writeln!(&mut file, "pretrunc {i}").unwrap();
            }

            file.flush().unwrap();
            sleep_500_millis().await; // The writes must be observed before truncating

            file.set_len(0).unwrap();
            file.seek(std::io::SeekFrom::Start(0)).unwrap();

            file.sync_all().unwrap();
            sleep_500_millis().await; // The truncate must be observed before writing again

            for i in 0..n {
                writeln!(&mut file, "posttrunc {i}").unwrap();
            }

            file.flush().unwrap();
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
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            for i in 0..n {
                writeln!(&mut file, "prerot {i}").unwrap();
            }

            file.flush().unwrap();
            sleep_500_millis().await; // The writes must be observed before rotating

            fs::rename(&path, archive_path).expect("could not rename");
            file.sync_all().unwrap();

            let mut file = File::create(&path).unwrap();

            file.sync_all().unwrap();
            sleep_500_millis().await; // The rotation must be observed before writing again

            for i in 0..n {
                writeln!(&mut file, "postrot {i}").unwrap();
            }

            file.flush().unwrap();
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
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            let mut file1 = File::create(&path1).unwrap();
            let mut file2 = File::create(&path2).unwrap();
            let mut file3 = File::create(&path3).unwrap();
            let mut file4 = File::create(&path4).unwrap();

            for i in 0..n {
                writeln!(&mut file1, "1 {i}").unwrap();
                writeln!(&mut file2, "2 {i}").unwrap();
                writeln!(&mut file3, "3 {i}").unwrap();
                writeln!(&mut file4, "4 {i}").unwrap();
            }
            file1.flush().unwrap();
            file2.flush().unwrap();
            file3.flush().unwrap();
            file4.flush().unwrap();

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
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
            let mut file1 = File::create(&path1).unwrap();
            let mut file2 = File::create(&path2).unwrap();

            for i in 0..n {
                writeln!(&mut file1, "1 {i}").unwrap();
                writeln!(&mut file2, "2 {i}").unwrap();
            }

            file1.flush().unwrap();
            file2.flush().unwrap();
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
            let received =
                run_file_source(&config, true, acks, LogNamespace::Legacy, None, async {
                    let mut file = File::create(&path).unwrap();

                    writeln!(&mut file, "hello there").unwrap();
                    file.flush().unwrap();

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
            let received =
                run_file_source(&config, true, acks, LogNamespace::Legacy, None, async {
                    let mut file = File::create(&path).unwrap();

                    writeln!(&mut file, "hello there").unwrap();
                    file.flush().unwrap();

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
            let received =
                run_file_source(&config, true, acks, LogNamespace::Legacy, None, async {
                    let mut file = File::create(&path).unwrap();

                    writeln!(&mut file, "hello there").unwrap();

                    file.flush().unwrap();
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

    #[tokio::test]
    async fn file_start_position_server_restart_acknowledged() {
        file_start_position_server_restart(Acks).await
    }

    #[tokio::test]
    async fn file_start_position_server_restart_no_acknowledge() {
        file_start_position_server_restart(NoAcks).await
    }

    async fn file_start_position_server_restart(acking: AckingMode) {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();
        writeln!(&mut file, "zeroth line").unwrap();
        file.flush().unwrap();

        // First time server runs it picks up existing lines.
        {
            let received =
                run_file_source(&config, true, acking, LogNamespace::Legacy, None, async {
                    sleep_500_millis().await;
                    writeln!(&mut file, "first line").unwrap();
                    file.flush().unwrap();
                    sleep_500_millis().await;
                })
                .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["zeroth line", "first line"]);
        }
        // Restart server, read file from checkpoint.
        {
            let received =
                run_file_source(&config, true, acking, LogNamespace::Legacy, None, async {
                    sleep_500_millis().await;
                    writeln!(&mut file, "second line").unwrap();
                    file.flush().unwrap();
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
            let received =
                run_file_source(&config, false, acking, LogNamespace::Legacy, None, async {
                    sleep_500_millis().await;
                    writeln!(&mut file, "third line").unwrap();
                    file.flush().unwrap();
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
        file.flush().unwrap();

        // First time server runs it picks up existing lines.
        let received = run_file_source(
            &config,
            false,
            Unfinalized,
            LogNamespace::Legacy,
            None,
            sleep(Duration::from_secs(5)),
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
            None,
            sleep(Duration::from_secs(5)),
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
            writeln!(&mut file, "Here's a line for you: {i}").unwrap();
        }
        file.flush().unwrap();

        // First time server runs it should pick up a bunch of lines
        let received = run_file_source(
            &config,
            true,
            Acks,
            LogNamespace::Legacy,
            None,
            // shutdown signal is sent after this duration
            sleep_500_millis(),
        )
        .await;
        let lines = extract_messages_string(received);

        // ...but not all the lines; if the first run processed the entire file, we may not hit the
        // bug we're testing for, which happens if the finalizer stream exits on shutdown with pending acks
        assert!(lines.len() < line_count);

        // Restart the server, and it should read the rest without duplicating any.
        // Use the event counter to drain rx continuously (removing backpressure so
        // the file server can read all remaining lines without being stalled), then
        // trigger shutdown once all expected events have been received.
        let remaining = line_count - lines.len();
        let event_count = Arc::new(AtomicUsize::new(0));
        let received = run_file_source(
            &config,
            true,
            Acks,
            LogNamespace::Legacy,
            Some(Arc::clone(&event_count)),
            async {
                wait_for_atomic_usize_timeout_ms(
                    Arc::clone(&event_count),
                    |n| n >= remaining,
                    5_000,
                )
                .await;
            },
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
            let received =
                run_file_source(&config, true, acking, LogNamespace::Legacy, None, async {
                    let mut file = File::create(&path).unwrap();
                    writeln!(&mut file, "first line").unwrap();
                    file.flush().unwrap();
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
            let received =
                run_file_source(&config, false, acking, LogNamespace::Legacy, None, async {
                    let mut file = File::create(&path).unwrap();
                    writeln!(&mut file, "second line").unwrap();
                    file.flush().unwrap();
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

        before_file.sync_all().unwrap();
        after_file.sync_all().unwrap();

        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            sleep_500_millis().await;
            writeln!(&mut before_file, "second line").unwrap();
            writeln!(&mut after_file, "_second line").unwrap();

            before_file.flush().unwrap();
            after_file.flush().unwrap();
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
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            writeln!(&mut file, "short").unwrap();
            writeln!(&mut file, "this is too long").unwrap();
            writeln!(&mut file, "11 eleven11").unwrap();
            let super_long = "This line is super long and will take up more space than BufReader's internal buffer, just to make sure that everything works properly when multiple read calls are involved".repeat(10000);
            writeln!(&mut file, "{super_long}").unwrap();
            writeln!(&mut file, "exactly 10").unwrap();
            writeln!(&mut file, "it can end on a line that's too long").unwrap();

            file.flush().unwrap();
            sleep_500_millis().await;
            sleep_500_millis().await;

            writeln!(&mut file, "and then continue").unwrap();
            writeln!(&mut file, "last short").unwrap();
            file.flush().unwrap();

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
            multi_line_timeout: 25,
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let event_count = Arc::new(AtomicUsize::new(0));
        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            Some(Arc::clone(&event_count)),
            async {
                let mut file = File::create(&path).unwrap();

                // Write all lines through the second "INFO hello". Events 1-4
                // are emitted immediately by EndExclude; event 5 ("INFO hello"
                // standalone) requires the 25ms timeout to fire.
                writeln!(&mut file, "leftover foo").unwrap();
                writeln!(&mut file, "INFO hello").unwrap();
                writeln!(&mut file, "INFO goodbye").unwrap();
                writeln!(&mut file, "part of goodbye").unwrap();
                writeln!(&mut file, "INFO hi again").unwrap();
                writeln!(&mut file, "and some more").unwrap();
                writeln!(&mut file, "INFO hello").unwrap();
                file.flush().unwrap();

                // Block until event 5 is observed: the timeout fired and
                // "INFO hello" was emitted before we write "too slow".
                wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 5, 500).await;

                writeln!(&mut file, "too slow").unwrap();
                writeln!(&mut file, "INFO doesn't have").unwrap();
                writeln!(&mut file, "to be INFO in").unwrap();
                writeln!(&mut file, "the middle").unwrap();
                file.flush().unwrap();

                // Wait for events 6 ("too slow") and 7 ("INFO doesn't have")
                // before triggering shutdown.
                wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 7, 500).await;
            },
        )
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
                timeout_ms: Duration::from_millis(25),
            }),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let event_count = Arc::new(AtomicUsize::new(0));
        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            Some(Arc::clone(&event_count)),
            async {
                let mut file = File::create(&path).unwrap();

                // Write all lines through the second "INFO hello". Events 1-4
                // are emitted immediately by EndExclude; event 5 ("INFO hello"
                // standalone) requires the 25ms timeout to fire.
                writeln!(&mut file, "leftover foo").unwrap();
                writeln!(&mut file, "INFO hello").unwrap();
                writeln!(&mut file, "INFO goodbye").unwrap();
                writeln!(&mut file, "part of goodbye").unwrap();
                writeln!(&mut file, "INFO hi again").unwrap();
                writeln!(&mut file, "and some more").unwrap();
                writeln!(&mut file, "INFO hello").unwrap();
                file.flush().unwrap();

                // Block until event 5 is observed: the timeout fired and
                // "INFO hello" was emitted before we write "too slow".
                wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 5, 500).await;

                writeln!(&mut file, "too slow").unwrap();
                writeln!(&mut file, "INFO doesn't have").unwrap();
                writeln!(&mut file, "to be INFO in").unwrap();
                writeln!(&mut file, "the middle").unwrap();
                file.flush().unwrap();

                // Wait for events 6 ("too slow") and 7 ("INFO doesn't have")
                // before triggering shutdown.
                wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 7, 500).await;
            },
        )
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

        file.sync_all().unwrap();

        // Read and aggregate existing lines. wait_shutdown=true ensures the
        // checkpoint is fully written to disk before the second run reads it.
        let received = run_file_source(
            &config,
            true,
            Acks,
            LogNamespace::Legacy,
            None,
            sleep_500_millis(),
        )
        .await;

        assert_eq!(received[0].as_log()["offset"], 0.into());

        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["INFO hello\npart of hello"]);

        // After restart, we should not see any part of the previously aggregated lines
        let received_after_restart =
            run_file_source(&config, false, Acks, LogNamespace::Legacy, None, async {
                writeln!(&mut file, "INFO goodbye").unwrap();
                file.flush().unwrap();
                sleep_500_millis().await;
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

        writeln!(&mut older, "hello i am the old file").unwrap();
        writeln!(&mut older, "i have been around a while").unwrap();
        writeln!(&mut older, "you can read newer files at the same time").unwrap();
        older.sync_all().unwrap();

        let newer_path = dir.path().join("a_newer_file");
        let mut newer = File::create(&newer_path).unwrap();

        writeln!(&mut newer, "and i am the new file").unwrap();
        writeln!(&mut newer, "this should be interleaved with the old one").unwrap();
        writeln!(&mut newer, "which is fine because we want fairness").unwrap();
        newer.sync_all().unwrap();

        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            None,
            sleep_500_millis(),
        )
        .await;

        let received = extract_messages_value(received);

        let old_first = vec![
            "hello i am the old file".into(),
            "and i am the new file".into(),
            "i have been around a while".into(),
            "this should be interleaved with the old one".into(),
            "you can read newer files at the same time".into(),
            "which is fine because we want fairness".into(),
        ];
        let new_first: Vec<_> = old_first
            .chunks(2)
            .flat_map(|chunk| chunk.iter().rev().cloned().collect::<Vec<_>>())
            .collect();

        if received[0] == old_first[0] {
            assert_eq!(received, old_first);
        } else {
            assert_eq!(received, new_first);
        }
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
        older.sync_all().unwrap();

        // Sleep to ensure the creation timestamps are different
        sleep_500_millis().await;

        let newer_path = dir.path().join("a_newer_file");
        let mut newer = File::create(&newer_path).unwrap();
        newer.sync_all().unwrap();

        writeln!(&mut older, "hello i am the old file").unwrap();
        writeln!(&mut older, "i have been around a while").unwrap();
        writeln!(&mut older, "you should definitely read all of me first").unwrap();
        older.flush().unwrap();

        writeln!(&mut newer, "i'm new").unwrap();
        writeln!(&mut newer, "hopefully you read all the old stuff first").unwrap();
        writeln!(&mut newer, "because otherwise i'm not going to make sense").unwrap();
        newer.flush().unwrap();

        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            None,
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
        file.sync_all().unwrap();

        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            sleep_500_millis().await;

            write!(&mut file, "i am not a full line").unwrap();

            file.flush().unwrap();
            // Longer than the EOF timeout
            sleep_500_millis().await;

            writeln!(&mut file, " until now").unwrap();

            file.flush().unwrap();
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
            None,
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
            None,
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
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            write!(&mut file, "hello i am a line\r\n").unwrap();
            write!(&mut file, "and i am too\r\n").unwrap();
            write!(&mut file, "CRLF is how we end\r\n").unwrap();
            write!(&mut file, "please treat us well\r\n").unwrap();

            file.flush().unwrap();
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

    // Regression test for https://github.com/vectordotdev/vector/issues/24027
    // Tests that multi-character delimiters (like \r\n) are correctly handled when
    // split across buffer boundaries. Without the fix, events would be merged together.
    #[tokio::test]
    async fn test_multi_char_delimiter_split_across_buffer_boundary() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            line_delimiter: "\r\n".to_string(),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            sleep_500_millis().await;

            // Create data where \r\n is split at 8KB buffer boundary
            // This reproduces the exact scenario that caused data corruption:
            // - Event 1 ends with \r at byte 8191
            // - The \n appears at byte 8192 (right at the buffer boundary)
            // - Without the fix, Event 1 and Event 2 would be merged

            let buffer_size = 8192;

            // Event 1: Position \r\n to split at first boundary
            let event1_prefix = "Event 1: ";
            let padding1_len = buffer_size - event1_prefix.len() - 1; // -1 for the \r
            write!(&mut file, "{}", event1_prefix).unwrap();
            file.write_all(&vec![b'X'; padding1_len]).unwrap();
            write!(&mut file, "\r\n").unwrap(); // \r at byte 8191, \n at byte 8192

            // Event 2: Position \r\n to split at second boundary
            let event2_prefix = "Event 2: ";
            let padding2_len = buffer_size - event2_prefix.len() - 1;
            write!(&mut file, "{}", event2_prefix).unwrap();
            file.write_all(&vec![b'Y'; padding2_len]).unwrap();
            write!(&mut file, "\r\n").unwrap(); // \r at byte 16383, \n at byte 16384

            // Event 3: Normal line without boundary split
            write!(&mut file, "Event 3: Final\r\n").unwrap();

            sleep_500_millis().await;
        })
        .await;

        let messages = extract_messages_value(received);

        // The bug would cause Events 1 and 2 to be merged into a single message
        assert_eq!(
            messages.len(),
            3,
            "Should receive exactly 3 separate events (bug would merge them)"
        );

        // Verify each event is correctly separated and starts with expected prefix
        let msg0 = messages[0].to_string_lossy();
        let msg1 = messages[1].to_string_lossy();
        let msg2 = messages[2].to_string_lossy();

        assert!(
            msg0.starts_with("Event 1: "),
            "First event should start with 'Event 1: ', got: {}",
            msg0
        );
        assert!(
            msg1.starts_with("Event 2: "),
            "Second event should start with 'Event 2: ', got: {}",
            msg1
        );
        assert_eq!(msg2, "Event 3: Final");

        // Ensure no event contains embedded CR/LF (sign of incorrect merging)
        for (i, msg) in messages.iter().enumerate() {
            let msg_str = msg.to_string_lossy();
            assert!(
                !msg_str.contains('\r'),
                "Event {} should not contain embedded \\r",
                i
            );
            assert!(
                !msg_str.contains('\n'),
                "Event {} should not contain embedded \\n",
                i
            );
        }
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
        let received = run_file_source(&config, false, Acks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            for i in 0..n {
                writeln!(&mut file, "{i}").unwrap();
            }
            file.flush().unwrap();
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

    // --- Idle-watching tests ---------------------------------------------
    //
    // These exercise the fix for https://github.com/vectordotdev/vector/issues/3567:
    // Vector previously held an open file handle for every matched file for
    // as long as it existed on disk, even files excluded by `ignore_older`
    // or long past EOF with no new writes. `idle_timeout_secs` (runtime) and
    // the startup fast-path in `FileWatcher::new` (see
    // lib/file-source/src/file_watcher/mod.rs) address this, independently
    // of `file_discovery_mode` -- these tests use the default `polling`
    // discovery mode (see the separate `notify_discovery` module below for
    // tests specifically covering the `notify` discovery mode, which is an
    // orthogonal concern: `notify` speeds up *finding* files, idle_timeout
    // stops *already-found* files from holding a handle open). These are
    // end-to-end tests through the full `file_source`/`FileServer` pipeline,
    // asserting on observable behavior (events received, and correct
    // resumption) rather than internal `FileWatcher` state, complementing
    // the lower-level state-transition tests in
    // lib/file-source/src/file_watcher/tests/mod.rs.

    #[tokio::test]
    async fn idle_timeout_closes_handle_and_resumes_on_new_data() {
        let n = 3;
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            // Aggressively short idle timeout so the watcher goes idle
            // quickly within the test's time budget.
            idle_timeout_secs: Some(0),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let counter = Arc::new(AtomicUsize::new(0));
        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            Some(Arc::clone(&counter)),
            async {
                let mut file = File::create(&path).unwrap();
                for i in 0..n {
                    writeln!(&mut file, "first-batch {i}").unwrap();
                }
                file.flush().unwrap();

                // Wait for the first batch to be received...
                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= n, 5_000).await;

                // ...then wait long enough for the watcher to reach EOF, sit
                // idle past `idle_timeout_secs: 0`, and be deactivated
                // (handle closed) by `FileServer`. A few glob-rescan/read
                // cycles at the 100ms `glob_minimum_cooldown_ms` used by
                // `test_default_file_config` is more than enough.
                sleep(Duration::from_millis(750)).await;

                // Now write more data. If the idle->active transition and
                // checkpoint-resume work correctly, this must be picked up
                // and read starting from exactly where we left off (no
                // duplicate replay of the first batch, no gap).
                for i in 0..n {
                    writeln!(&mut file, "second-batch {i}").unwrap();
                }
                file.flush().unwrap();

                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 2 * n, 5_000).await;
            },
        )
        .await;

        let lines = extract_messages_string(received);
        assert_eq!(lines.len(), 2 * n);
        for i in 0..n {
            assert_eq!(lines[i], format!("first-batch {i}"));
        }
        for i in 0..n {
            assert_eq!(lines[n + i], format!("second-batch {i}"));
        }
    }

    #[tokio::test]
    async fn idle_old_fully_read_file_is_not_reread_on_restart() {
        // A file that is: (a) older than `ignore_older_secs`, and (b) whose
        // on-disk size already matches its stored checkpoint (nothing new to
        // read) must, per the startup fast-path in `FileWatcher::new`, be
        // tracked without ever being opened. Observably: it must produce no
        // events on a restart, and the data dir's checkpoint must be
        // unaffected (no re-read from the beginning).
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();
        writeln!(&mut file, "only line").unwrap();
        file.flush().unwrap();

        // First run: read the one line and checkpoint it.
        {
            let received =
                run_file_source(&config, true, Acks, LogNamespace::Legacy, None, async {
                    sleep_500_millis().await;
                })
                .await;
            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["only line"]);
        }

        // Second run: `ignore_older_secs` set aggressively low so `file`
        // (unmodified since the first run, so at least a little bit old by
        // now) is excluded. Combined with the checkpoint from the first run
        // matching its actual size, `file` must take the idle fast-path at
        // startup and yield no new events for it -- but must NOT lose its
        // checkpoint or get treated as newly-discovered. A second, freshly
        // written file is included in the same run so the harness's
        // component-compliance check (which requires at least one event) has
        // something to observe, letting us assert on `file` specifically
        // being absent from the output rather than the run producing nothing
        // at all.
        {
            let other_path = dir.path().join("other_file");
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                ignore_older_secs: Some(1),
                ..test_default_file_config(&dir)
            };
            let counter = Arc::new(AtomicUsize::new(0));
            let received = run_file_source(
                &config,
                true,
                Acks,
                LogNamespace::Legacy,
                Some(Arc::clone(&counter)),
                async {
                    let mut other_file = File::create(&other_path).unwrap();
                    writeln!(&mut other_file, "fresh line").unwrap();
                    other_file.flush().unwrap();
                    wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 1, 5_000).await;
                },
            )
            .await;
            let lines = extract_messages_string(received);
            assert_eq!(
                lines,
                vec!["fresh line"],
                "old, fully-checkpointed `file` must not be re-read, \
                 only the newly written `other_file` should produce events"
            );
        }
    }

    #[tokio::test]
    async fn idle_file_deletion_is_handled_without_reopening() {
        // A file that goes idle (handle closed) and is then deleted must be
        // unwatched just like an actively-open file that gets deleted --
        // without ever needing to reopen it to notice the deletion.
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            idle_timeout_secs: Some(0),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let counter = Arc::new(AtomicUsize::new(0));
        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            Some(Arc::clone(&counter)),
            async {
                let mut file = File::create(&path).unwrap();
                writeln!(&mut file, "hello").unwrap();
                file.flush().unwrap();
                drop(file);

                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 1, 5_000).await;

                // Give it time to go idle (handle closed) before deleting.
                sleep(Duration::from_millis(750)).await;

                std::fs::remove_file(&path).unwrap();

                // Give the glob-rescan loop a chance to notice the deletion
                // and unwatch the file; there's no new event to wait on
                // here, so just sleep a bit past a few rescan cycles.
                sleep(Duration::from_millis(750)).await;
            },
        )
        .await;

        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["hello"]);
    }

    #[tokio::test]
    async fn idle_file_rotation_reads_new_file_not_stale_offset() {
        // A file that goes idle, then gets rotated (renamed away, replaced
        // by a new file at the same path) must pick up the *new* file's
        // content from the correct (fresh) position, not silently resume
        // reading into the new file from the old file's stale offset. This
        // relies on fingerprint-based identity in `FileServer` plus
        // `FileWatcher::update_path`'s dev/inode re-verification on
        // reactivation.
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            idle_timeout_secs: Some(0),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let archive_path = dir.path().join("file.1");
        let counter = Arc::new(AtomicUsize::new(0));
        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            Some(Arc::clone(&counter)),
            async {
                let mut file = File::create(&path).unwrap();
                writeln!(&mut file, "old file content").unwrap();
                file.flush().unwrap();

                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 1, 5_000).await;

                // Let it go idle.
                sleep(Duration::from_millis(750)).await;

                // Rotate: move the old file aside, create a new,
                // content-different file at the same path.
                fs::rename(&path, &archive_path).expect("could not rename");
                let mut new_file = File::create(&path).unwrap();
                writeln!(&mut new_file, "brand new file content").unwrap();
                new_file.flush().unwrap();

                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 2, 5_000).await;
            },
        )
        .await;

        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["old file content", "brand new file content"]);
    }

    #[tokio::test]
    async fn idle_file_rotation_behind_narrow_glob_reads_new_file_not_stale_offset() {
        // Same scenario as `idle_file_rotation_reads_new_file_not_stale_offset`, but with an
        // `include` glob narrow enough that the archived file does *not* match it (a common
        // setup, e.g. `*.log` with rotated files renamed to `*.log.1`). The idle watcher has no
        // descriptor to pin the old inode, so it must locate that inode by identity in the parent
        // directory before reopening; otherwise an append through the writer's retained
        // descriptor would be lost and the replacement file could inherit a stale offset.
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*.log")],
            idle_timeout_secs: Some(0),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("app.log");
        let archive_path = dir.path().join("app.log.1"); // does NOT match `*.log`
        let counter = Arc::new(AtomicUsize::new(0));
        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            Some(Arc::clone(&counter)),
            async {
                let mut file = File::create(&path).unwrap();
                writeln!(&mut file, "old file content").unwrap();
                file.flush().unwrap();

                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 1, 5_000).await;

                // Let it go idle (handle closed).
                sleep(Duration::from_millis(750)).await;

                // Rotate: move the old file to a path outside the `include`
                // glob, then create a new, content-different file at the
                // original path.
                fs::rename(&path, &archive_path).expect("could not rename");
                let mut new_file = File::create(&path).unwrap();
                writeln!(&mut new_file, "brand new file content").unwrap();
                new_file.flush().unwrap();

                // The writer still has the rotated inode open. This data must be read from the
                // archive even though that path is outside the include glob.
                writeln!(&mut file, "late old file content").unwrap();
                file.flush().unwrap();

                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 3, 5_000).await;

                // Rotate the already archived inode again. This exercises the case where the
                // watcher is already tracking a path outside the glob: the next notify event
                // must still relocate that inode instead of abandoning it after the first move.
                sleep(Duration::from_millis(750)).await;
                let second_archive_path = dir.path().join("app.log.2");
                fs::rename(&archive_path, &second_archive_path).expect("could not rename again");
                writeln!(&mut file, "late old file content after second rotation").unwrap();
                file.flush().unwrap();

                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 4, 5_000).await;
            },
        )
        .await;

        let mut lines = extract_messages_string(received);
        lines.sort();
        assert_eq!(
            lines,
            vec![
                "brand new file content",
                "late old file content",
                "late old file content after second rotation",
                "old file content",
            ]
        );
    }

    #[tokio::test]
    async fn idle_outside_glob_replacement_is_not_reopened() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*.log")],
            idle_timeout_secs: Some(0),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("app.log");
        let archive_path = dir.path().join("app.log.1");
        let counter = Arc::new(AtomicUsize::new(0));
        let received = run_file_source(
            &config,
            false,
            NoAcks,
            LogNamespace::Legacy,
            Some(Arc::clone(&counter)),
            async {
                let mut old_file = File::create(&path).unwrap();
                writeln!(&mut old_file, "old file content").unwrap();
                old_file.flush().unwrap();
                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 1, 5_000).await;

                sleep(Duration::from_millis(750)).await;
                fs::rename(&path, &archive_path).unwrap();

                let mut new_file = File::create(&path).unwrap();
                writeln!(&mut new_file, "new file content").unwrap();
                new_file.flush().unwrap();
                writeln!(&mut old_file, "late old file content").unwrap();
                old_file.flush().unwrap();
                wait_for_atomic_usize_timeout_ms(Arc::clone(&counter), |c| c >= 3, 5_000).await;

                // Ensure the old inode has been found outside the glob and its idle handle closed
                // before removing it. The replacement below must not be attached to that watcher.
                sleep(Duration::from_millis(750)).await;
                drop(old_file);
                fs::remove_file(&archive_path).unwrap();
                let mut replacement = File::create(&archive_path).unwrap();
                writeln!(&mut replacement, "unrelated replacement").unwrap();
                replacement.flush().unwrap();

                sleep(Duration::from_millis(750)).await;
            },
        )
        .await;

        let lines = extract_messages_string(received);
        assert_eq!(lines.len(), 3);
        assert!(
            !lines.iter().any(|line| line == "unrelated replacement"),
            "a replacement at the old outside-glob path must not be reopened"
        );
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum AckingMode {
        NoAcks,      // No acknowledgement handling and no finalization
        Unfinalized, // Acknowledgement handling but no finalization
        Acks,        // Full acknowledgements and proper finalization
    }
    use AckingMode::*;
    use vector_lib::lookup::OwnedTargetPath;

    async fn run_file_source(
        config: &FileConfig,
        wait_shutdown: bool,
        acking_mode: AckingMode,
        log_namespace: LogNamespace,
        // When `Some`, events are relayed through an unbounded channel and the
        // counter is incremented for each event received.  The inner future can
        // call `wait_for_atomic_usize` on this counter to gate writes on
        // observed events instead of relying on wall-clock sleeps.
        event_counter: Option<Arc<AtomicUsize>>,
        inner: impl Future<Output = ()>,
    ) -> Vec<Event> {
        assert_source_compliance(&FILE_SOURCE_TAGS, async move {
            let (tx, rx) = match acking_mode {
                Acks => {
                    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);
                    (tx, rx.boxed())
                }
                Unfinalized => {
                    // Use Rejected so that events are finalized but checkpoints
                    // are NOT updated (only Delivered triggers checkpoint updates).
                    // This avoids a race where the default Delivered status on drop
                    // could leak checkpoint writes into the next run.
                    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Rejected);
                    (tx, rx.boxed())
                }
                NoAcks => {
                    let (tx, rx) = SourceSender::new_test();
                    (tx, rx.boxed())
                }
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

            let result = if let Some(counter) = event_counter {
                // Relay mode: a background task forwards events and increments
                // the counter so `inner` can observe them without arbitrary sleeps.
                let (relay_tx, mut relay_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
                tokio::spawn(async move {
                    let mut rx = rx;
                    while let Some(event) = rx.next().await {
                        counter.fetch_add(1, Ordering::SeqCst);
                        relay_tx.send(event).ok(); // receiver gone means pipeline is shutting down
                    }
                });

                inner.await;
                drop(trigger_shutdown);

                timeout(Duration::from_secs(5), async move {
                    let mut events = Vec::new();
                    while let Some(event) = relay_rx.recv().await {
                        events.push(event);
                    }
                    events
                })
                .await
                .expect("Unclosed channel: may indicate file-server could not shutdown gracefully.")
            } else {
                inner.await;
                drop(trigger_shutdown);

                if acking_mode == Unfinalized {
                    rx.take_until(tokio::time::sleep(Duration::from_secs(5)))
                        .collect::<Vec<_>>()
                        .await
                } else {
                    timeout(Duration::from_secs(5), rx.collect::<Vec<_>>())
                        .await
                        .expect(
                            "Unclosed channel: may indicate file-server could not shutdown gracefully.",
                        )
                }
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

    /// Tests covering `file_discovery_mode: notify`, the OS-level filesystem-event-driven
    /// discovery mode. These reuse `run_file_source`/`test_default_file_config` from above but
    /// set a very long `glob_minimum_cooldown_ms`/`reconcile_interval_secs`, so that the
    /// periodic backstop reconciliation pass cannot plausibly fire within the test's timeout.
    /// If a test still observes prompt discovery/read behavior under those settings, that
    /// behavior must be coming from the notify event path, not the polling fallback -- this is
    /// what distinguishes these tests from the equivalent polling-mode tests above.
    mod notify_discovery {
        use super::*;

        fn test_notify_file_config(dir: &tempfile::TempDir) -> file::FileConfig {
            file::FileConfig {
                file_discovery_mode: FileDiscoveryModeConfig::Notify,
                // Deliberately huge: if the backstop reconciliation pass were doing the work in
                // these tests, they would time out (the tests use short, second-scale timeouts)
                // well before this interval ever elapses.
                reconcile_interval: Duration::from_secs(3600),
                // Likewise huge and, in `Notify` mode, unused for discovery timing: set high to
                // double-check no code path (including the notify-event debounce window, which
                // is its own fixed, small constant -- NOTIFY_EVENT_DEBOUNCE -- specifically so it
                // can't inherit an unrelated-in-intent large value like this one) is silently
                // relying on it as a polling or debounce interval.
                glob_minimum_cooldown_ms: Duration::from_secs(3600),
                ..test_default_file_config(dir)
            }
        }

        /// (a) A new file appearing after startup is picked up promptly via a create event, not
        /// a fixed polling interval that -- per `test_notify_file_config` -- is set to an hour.
        #[tokio::test]
        async fn new_file_discovered_promptly_via_event() {
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                ..test_notify_file_config(&dir)
            };

            let path = dir.path().join("new_file");
            let event_count = Arc::new(AtomicUsize::new(0));
            let received = run_file_source(
                &config,
                false,
                NoAcks,
                LogNamespace::Legacy,
                Some(Arc::clone(&event_count)),
                async {
                    // The file doesn't exist yet at FileServer startup.
                    let mut file = File::create(&path).unwrap();
                    writeln!(&mut file, "hello from a brand new file").unwrap();
                    file.flush().unwrap();

                    // If this resolves, discovery + read happened well within the (hour-long)
                    // fallback reconcile interval, i.e. via the notify event path.
                    wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 1, 5_000)
                        .await;
                },
            )
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["hello from a brand new file"]);
        }

        /// (b) A write to an already-tracked file triggers a prompt read via a modify event.
        #[tokio::test]
        async fn write_to_existing_file_triggers_prompt_read() {
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                ..test_notify_file_config(&dir)
            };

            let path = dir.path().join("existing_file");
            File::create(&path).unwrap();

            let event_count = Arc::new(AtomicUsize::new(0));
            let received = run_file_source(
                &config,
                false,
                NoAcks,
                LogNamespace::Legacy,
                Some(Arc::clone(&event_count)),
                async {
                    // Give the file server a brief moment to complete startup and establish its
                    // watch before we write, but well under the reconcile interval.
                    sleep(Duration::from_millis(200)).await;

                    let mut file = std::fs::OpenOptions::new()
                        .append(true)
                        .open(&path)
                        .unwrap();
                    writeln!(&mut file, "a new line was written").unwrap();
                    file.flush().unwrap();

                    wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 1, 5_000)
                        .await;
                },
            )
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["a new line was written"]);
        }

        /// (d) Rotation (rename) is still handled correctly under notify-based discovery: the
        /// fingerprint (not the path) identifies the file being tailed, and post-rotation writes
        /// to the recreated path are picked up as a new file.
        #[tokio::test]
        async fn rotation_handled_correctly() {
            let n = 5;
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                ..test_notify_file_config(&dir)
            };

            let path = dir.path().join("file");
            let archive_path = dir.path().join("file.old");
            let received =
                run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
                    let mut file = File::create(&path).unwrap();
                    for i in 0..n {
                        writeln!(&mut file, "prerot {i}").unwrap();
                    }
                    file.flush().unwrap();
                    sleep(Duration::from_millis(500)).await;

                    fs::rename(&path, &archive_path).expect("could not rename");
                    file.sync_all().unwrap();

                    let mut file = File::create(&path).unwrap();
                    file.sync_all().unwrap();
                    sleep(Duration::from_millis(500)).await;

                    for i in 0..n {
                        writeln!(&mut file, "postrot {i}").unwrap();
                    }
                    file.flush().unwrap();
                    sleep(Duration::from_millis(500)).await;
                })
                .await;

            let mut i = 0;
            let mut pre_rot = true;
            for event in received {
                let line = event.as_log()[log_schema().message_key().unwrap().to_string()]
                    .to_string_lossy();
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

        /// (c) No file handle is held for files that never receive any activity: unlike the
        /// polling model (which re-fingerprints, and therefore re-opens, every matched file on
        /// every cooldown tick), notify-driven discovery only opens files at startup (for the
        /// initial scan) or in response to a create/modify event. A file that sits untouched
        /// after being discovered is read to EOF once and then left alone -- the read loop does
        /// not touch it again absent a new event, so no repeated open/fingerprint cost is paid.
        ///
        /// This test can't directly inspect the process's open file descriptor table in a
        /// portable way, so instead it asserts on the behavior that open-handle-avoidance is
        /// meant to buy us: a large number of untouched files do not prevent, or measurably
        /// delay, prompt discovery and reading of one actively-written file. Under the old
        /// polling design this same scenario would still work, but would pay an O(n) glob +
        /// fingerprint cost on every single tick; here, with the reconcile interval set to an
        /// hour, that cost structurally cannot be paid within the test, so a prompt result
        /// demonstrates the write path isn't depending on scanning the inactive files at all.
        #[tokio::test]
        async fn inactive_files_do_not_block_prompt_discovery() {
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                ..test_notify_file_config(&dir)
            };

            // Create a bunch of files that will never be written to again.
            for i in 0..200 {
                let mut f = File::create(dir.path().join(format!("inactive_{i}"))).unwrap();
                writeln!(&mut f, "inactive content {i}").unwrap();
                f.flush().unwrap();
            }

            let active_path = dir.path().join("active_file");
            let event_count = Arc::new(AtomicUsize::new(0));
            let received = run_file_source(
                &config,
                false,
                NoAcks,
                LogNamespace::Legacy,
                Some(Arc::clone(&event_count)),
                async {
                    let mut file = File::create(&active_path).unwrap();
                    writeln!(&mut file, "active line").unwrap();
                    file.flush().unwrap();

                    // 200 pre-existing untouched files + 1 new active file. All 201 lines
                    // (200 inactive + 1 active) get read once during the startup scan / the
                    // active file's create event; we just need to see them all arrive promptly.
                    wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 201, 5_000)
                        .await;
                },
            )
            .await;

            let lines = extract_messages_string(received);
            assert!(lines.contains(&"active line".to_string()));
            assert_eq!(lines.len(), 201);
        }
    }
}
