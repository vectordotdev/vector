use std::{
    collections::{HashMap, HashSet},
    io::SeekFrom,
    path::PathBuf,
    process::Stdio,
    str::FromStr,
    sync::{Arc, LazyLock},
    time::Duration,
};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{StreamExt, poll, stream::BoxStream, task::Poll};
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use serde_json::{Error as JsonError, Value as JsonValue};
use snafu::{ResultExt, Snafu};
use tokio::{
    fs::{File, OpenOptions},
    io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::{Child, Command},
    sync::{Mutex, MutexGuard},
    time::sleep,
};
use tokio_util::codec::FramedRead;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{CharacterDelimitedDecoder, decoding::BoxedFramingError},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    finalizer::OrderedFinalizer,
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol, Registered,
    },
    lookup::{metadata_path, owned_value_path, path},
    schema::Definition,
};
use vrl::{
    event_path,
    value::{Kind, Value, kind::Collection},
};

use crate::{
    SourceSender,
    config::{
        DataType, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
        log_schema,
    },
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, LogEvent},
    internal_events::{
        EventsReceived, JournaldCheckpointFileOpenError, JournaldCheckpointSetError,
        JournaldInvalidRecordError, JournaldReadError, JournaldStartJournalctlError,
        StreamClosedError,
    },
    serde::bool_or_struct,
    shutdown::ShutdownSignal,
};

const BATCH_TIMEOUT: Duration = Duration::from_millis(10);

const CHECKPOINT_FILENAME: &str = "checkpoint.txt";
const CURSOR: &str = "__CURSOR";
const HOSTNAME: &str = "_HOSTNAME";
const MESSAGE: &str = "MESSAGE";
const SYSTEMD_UNIT: &str = "_SYSTEMD_UNIT";
const SOURCE_TIMESTAMP: &str = "_SOURCE_REALTIME_TIMESTAMP";
const RECEIVED_TIMESTAMP: &str = "__REALTIME_TIMESTAMP";

const BACKOFF_DURATION: Duration = Duration::from_secs(1);

static JOURNALCTL: LazyLock<PathBuf> = LazyLock::new(|| "journalctl".into());

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("journalctl failed to execute: {}", source))]
    JournalctlSpawn { source: io::Error },
    #[snafu(display("failed to parse output of `journalctl --version`: {:?}", output))]
    JournalctlParseVersion { output: String },
    #[snafu(display(
        "The unit {:?} is duplicated in both include_units and exclude_units",
        unit
    ))]
    DuplicatedUnit { unit: String },
    #[snafu(display(
        "The Journal field/value pair {:?}:{:?} is duplicated in both include_matches and exclude_matches.",
        field,
        value,
    ))]
    DuplicatedMatches { field: String, value: String },
    #[snafu(display(
        "`current_boot_only: false` not supported for systemd versions 250 through 257 (got {}).",
        systemd_version
    ))]
    AllBootsNotSupported { systemd_version: u32 },
}

type Matches = HashMap<String, HashSet<String>>;

/// Configuration for the `journald` source.
#[configurable_component(source("journald", "Collect logs from JournalD."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct JournaldConfig {
    /// Only include entries that appended to the journal after the entries have been read.
    #[serde(default)]
    pub since_now: bool,

    /// Only include entries that occurred after the current boot of the system.
    #[serde(default = "crate::serde::default_true")]
    pub current_boot_only: bool,

    /// A list of unit names to monitor.
    ///
    /// If empty or not present, all units are accepted.
    ///
    /// Unit names lacking a `.` have `.service` appended to make them a valid service unit name.
    ///
    /// **Note:** This option matches only the `_SYSTEMD_UNIT` field, which is narrower than `journalctl --unit`.
    /// Messages from systemd about unit lifecycle (start/stop) have `_SYSTEMD_UNIT=init.scope` and will not match.
    /// To capture these, explicitly include `init.scope` or use `include_matches` for finer control.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "ntpd", docs::examples = "sysinit.target"))]
    pub include_units: Vec<String>,

    /// A list of unit names to exclude from monitoring.
    ///
    /// Unit names lacking a `.` have `.service` appended to make them a valid service unit
    /// name.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "badservice", docs::examples = "sysinit.target"))]
    pub exclude_units: Vec<String>,

    /// A list of sets of field/value pairs to monitor.
    ///
    /// If empty or not present, all journal fields are accepted.
    ///
    /// If `include_units` is specified, it is merged into this list.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "The set of field values to match in journal entries that are to be included."
    ))]
    #[configurable(metadata(docs::examples = "matches_examples()"))]
    pub include_matches: Matches,

    /// A list of sets of field/value pairs that, if any are present in a journal entry,
    /// excludes the entry from this source.
    ///
    /// If `exclude_units` is specified, it is merged into this list.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "The set of field values to match in journal entries that are to be excluded."
    ))]
    #[configurable(metadata(docs::examples = "matches_examples()"))]
    pub exclude_matches: Matches,

    /// The directory used to persist file checkpoint positions.
    ///
    /// By default, the [global `data_dir` option][global_data_dir] is used.
    /// Make sure the running user has write permissions to this directory.
    ///
    /// If this directory is specified, then Vector will attempt to create it.
    ///
    /// [global_data_dir]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
    #[serde(default)]
    #[configurable(metadata(docs::examples = "/var/lib/vector"))]
    #[configurable(metadata(docs::human_name = "Data Directory"))]
    pub data_dir: Option<PathBuf>,

    /// A list of extra command line arguments to pass to `journalctl`.
    ///
    /// If specified, it is merged to the command line arguments as-is.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "--merge"))]
    pub extra_args: Vec<String>,

    /// The systemd journal is read in batches, and a checkpoint is set at the end of each batch.
    ///
    /// This option limits the size of the batch.
    #[serde(default = "default_batch_size")]
    #[configurable(metadata(docs::type_unit = "events"))]
    pub batch_size: usize,

    /// The full path of the `journalctl` executable.
    ///
    /// If not set, a search is done for the `journalctl` path.
    #[serde(default)]
    pub journalctl_path: Option<PathBuf>,

    /// The full path of the journal directory.
    ///
    /// If not set, `journalctl` uses the default system journal path.
    #[serde(default)]
    pub journal_directory: Option<PathBuf>,

    /// The [journal namespace][journal-namespace].
    ///
    /// This value is passed to `journalctl` through the [`--namespace` option][journalctl-namespace-option].
    /// If not set, `journalctl` uses the default namespace.
    ///
    /// [journal-namespace]: https://www.freedesktop.org/software/systemd/man/systemd-journald.service.html#Journal%20Namespaces
    /// [journalctl-namespace-option]: https://www.freedesktop.org/software/systemd/man/journalctl.html#--namespace=NAMESPACE
    #[serde(default)]
    pub journal_namespace: Option<String>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// Enables remapping the `PRIORITY` field from an integer to string value.
    ///
    /// Has no effect unless the value of the field is already an integer.
    #[serde(default)]
    #[configurable(
        deprecated = "This option has been deprecated, use the `remap` transform and `to_syslog_level` function instead."
    )]
    remap_priority: bool,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    /// Whether to emit the [__CURSOR field][cursor]. See also [sd_journal_get_cursor][get_cursor].
    ///
    /// [cursor]: https://www.freedesktop.org/software/systemd/man/latest/systemd.journal-fields.html#Address%20Fields
    /// [get_cursor]: https://www.freedesktop.org/software/systemd/man/latest/sd_journal_get_cursor.html
    #[serde(default = "crate::serde::default_false")]
    emit_cursor: bool,
}

const fn default_batch_size() -> usize {
    16
}

fn matches_examples() -> HashMap<String, Vec<String>> {
    HashMap::<_, _>::from_iter([
        (
            "_SYSTEMD_UNIT".to_owned(),
            vec!["sshd.service".to_owned(), "ntpd.service".to_owned()],
        ),
        ("_TRANSPORT".to_owned(), vec!["kernel".to_owned()]),
    ])
}

impl JournaldConfig {
    fn merged_include_matches(&self) -> Matches {
        Self::merge_units(&self.include_matches, &self.include_units)
    }

    fn merged_exclude_matches(&self) -> Matches {
        Self::merge_units(&self.exclude_matches, &self.exclude_units)
    }

    fn merge_units(matches: &Matches, units: &[String]) -> Matches {
        let mut matches = matches.clone();
        for unit in units {
            let entry = matches.entry(String::from(SYSTEMD_UNIT));
            entry.or_default().insert(fixup_unit(unit));
        }
        matches
    }

    /// Builds the `schema::Definition` for this source using the provided `LogNamespace`.
    fn schema_definition(&self, log_namespace: LogNamespace) -> Definition {
        let schema_definition = match log_namespace {
            LogNamespace::Vector => Definition::new_with_default_metadata(
                Kind::bytes().or_null(),
                [LogNamespace::Vector],
            ),
            LogNamespace::Legacy => Definition::new_with_default_metadata(
                Kind::object(Collection::empty()),
                [LogNamespace::Legacy],
            ),
        };

        let mut schema_definition = schema_definition
            .with_standard_vector_source_metadata()
            // for metadata that is added to the events dynamically through the Record
            .with_source_metadata(
                JournaldConfig::NAME,
                None,
                &owned_value_path!("metadata"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_source_metadata(
                JournaldConfig::NAME,
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            )
            .with_source_metadata(
                JournaldConfig::NAME,
                log_schema().host_key().cloned().map(LegacyKey::Overwrite),
                &owned_value_path!("host"),
                Kind::bytes().or_undefined(),
                Some("host"),
            );

        // for metadata that is added to the events dynamically through the Record
        if log_namespace == LogNamespace::Legacy {
            schema_definition = schema_definition.unknown_fields(Kind::bytes());
        }

        schema_definition
    }
}

impl Default for JournaldConfig {
    fn default() -> Self {
        Self {
            since_now: false,
            current_boot_only: true,
            include_units: vec![],
            exclude_units: vec![],
            include_matches: Default::default(),
            exclude_matches: Default::default(),
            data_dir: None,
            batch_size: default_batch_size(),
            journalctl_path: None,
            journal_directory: None,
            journal_namespace: None,
            extra_args: vec![],
            acknowledgements: Default::default(),
            remap_priority: false,
            log_namespace: None,
            emit_cursor: false,
        }
    }
}

impl_generate_config_from_default!(JournaldConfig);

type Record = HashMap<String, String>;

#[async_trait::async_trait]
#[typetag::serde(name = "journald")]
impl SourceConfig for JournaldConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        if self.remap_priority {
            warn!(
                "DEPRECATION, option `remap_priority` has been deprecated. Please use the `remap` transform and function `to_syslog_level` instead."
            );
        }

        let data_dir = cx
            .globals
            // source are only global, name can be used for subdir
            .resolve_and_make_data_subdir(self.data_dir.as_ref(), cx.key.id())?;

        if let Some(unit) = self
            .include_units
            .iter()
            .find(|unit| self.exclude_units.contains(unit))
        {
            let unit = unit.into();
            return Err(BuildError::DuplicatedUnit { unit }.into());
        }

        let include_matches = self.merged_include_matches();
        let exclude_matches = self.merged_exclude_matches();

        if let Some((field, value)) = find_duplicate_match(&include_matches, &exclude_matches) {
            return Err(BuildError::DuplicatedMatches { field, value }.into());
        }

        let mut checkpoint_path = data_dir;
        checkpoint_path.push(CHECKPOINT_FILENAME);

        let journalctl_path = self
            .journalctl_path
            .clone()
            .unwrap_or_else(|| JOURNALCTL.clone());

        let systemd_version = get_systemd_version_from_journalctl(&journalctl_path).await?;

        if !self.current_boot_only && (250..=257).contains(&systemd_version) {
            // https://github.com/vectordotdev/vector/issues/18068
            return Err(BuildError::AllBootsNotSupported { systemd_version }.into());
        }

        let starter = StartJournalctl::new(
            journalctl_path,
            systemd_version,
            self.journal_directory.clone(),
            self.journal_namespace.clone(),
            self.current_boot_only,
            self.since_now,
            self.extra_args.clone(),
        );

        let batch_size = self.batch_size;
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);
        let log_namespace = cx.log_namespace(self.log_namespace);

        Ok(Box::pin(
            JournaldSource {
                include_matches,
                exclude_matches,
                checkpoint_path,
                batch_size,
                remap_priority: self.remap_priority,
                out: cx.out,
                acknowledgements,
                starter,
                log_namespace,
                emit_cursor: self.emit_cursor,
            }
            .run_shutdown(cx.shutdown),
        ))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let schema_definition =
            self.schema_definition(global_log_namespace.merge(self.log_namespace));

        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

struct JournaldSource {
    include_matches: Matches,
    exclude_matches: Matches,
    checkpoint_path: PathBuf,
    batch_size: usize,
    remap_priority: bool,
    out: SourceSender,
    acknowledgements: bool,
    starter: StartJournalctl,
    log_namespace: LogNamespace,
    emit_cursor: bool,
}

impl JournaldSource {
    async fn run_shutdown(self, shutdown: ShutdownSignal) -> Result<(), ()> {
        let checkpointer = StatefulCheckpointer::new(self.checkpoint_path.clone())
            .await
            .map_err(|error| {
                emit!(JournaldCheckpointFileOpenError {
                    error,
                    path: self
                        .checkpoint_path
                        .to_str()
                        .unwrap_or("unknown")
                        .to_string(),
                });
            })?;

        let checkpointer = SharedCheckpointer::new(checkpointer);
        let finalizer = Finalizer::new(
            self.acknowledgements,
            checkpointer.clone(),
            shutdown.clone(),
        );

        self.run(checkpointer, finalizer, shutdown).await;

        Ok(())
    }

    async fn run(
        mut self,
        checkpointer: SharedCheckpointer,
        finalizer: Finalizer,
        mut shutdown: ShutdownSignal,
    ) {
        loop {
            if matches!(poll!(&mut shutdown), Poll::Ready(_)) {
                break;
            }

            info!("Starting journalctl.");
            let cursor = checkpointer.lock().await.cursor.clone();
            match self.starter.start(cursor.as_deref()) {
                Ok((stdout_stream, stderr_stream, running)) => {
                    if !self
                        .run_stream(stdout_stream, stderr_stream, &finalizer, shutdown.clone())
                        .await
                    {
                        return;
                    }
                    // Explicit drop to ensure it isn't dropped earlier.
                    drop(running);
                }
                Err(error) => {
                    emit!(JournaldStartJournalctlError { error });
                }
            }

            // journalctl process should never stop,
            // so it is an error if we reach here.
            tokio::select! {
                _ = &mut shutdown => break,
                _ = sleep(BACKOFF_DURATION) => (),
            }
        }
    }

    /// Process `journalctl` output until some error occurs.
    /// Return `true` if should restart `journalctl`.
    async fn run_stream<'a>(
        &'a mut self,
        mut stdout_stream: JournalStream,
        stderr_stream: JournalStream,
        finalizer: &'a Finalizer,
        mut shutdown: ShutdownSignal,
    ) -> bool {
        let bytes_received = register!(BytesReceived::from(Protocol::from("journald")));
        let events_received = register!(EventsReceived);

        // Spawn stderr handler task
        let stderr_handler = crate::spawn_in_current_span(Self::handle_stderr(stderr_stream));

        let batch_size = self.batch_size;
        let result = loop {
            let mut batch = Batch::new(self);

            // Start the timeout counter only once we have received a
            // valid and non-filtered event.
            while batch.events.is_empty() {
                let item = tokio::select! {
                    _ = &mut shutdown => {
                        stderr_handler.abort();
                        return false;
                    },
                    item = stdout_stream.next() => item,
                };
                if !batch.handle_next(item) {
                    stderr_handler.abort();
                    return true;
                }
            }

            let timeout = tokio::time::sleep(BATCH_TIMEOUT);
            tokio::pin!(timeout);

            for _ in 1..batch_size {
                tokio::select! {
                    _ = &mut timeout => break,
                    result = stdout_stream.next() => if !batch.handle_next(result) {
                        break;
                    }
                }
            }
            if let Some(x) = batch
                .finish(finalizer, &bytes_received, &events_received)
                .await
            {
                break x;
            }
        };

        stderr_handler.abort();
        result
    }

    /// Handle stderr stream from journalctl process
    async fn handle_stderr(mut stderr_stream: JournalStream) {
        while let Some(result) = stderr_stream.next().await {
            match result {
                Ok(line) => {
                    let line_str = String::from_utf8_lossy(&line);
                    let trimmed = line_str.trim();
                    if !trimmed.is_empty() {
                        warn!("Warning journalctl stderr: {trimmed}");
                    }
                }
                Err(err) => {
                    error!("Error reading journalctl stderr: {err}");
                    break;
                }
            }
        }
    }
}

struct Batch<'a> {
    events: Vec<LogEvent>,
    record_size: usize,
    exiting: Option<bool>,
    batch: Option<BatchNotifier>,
    receiver: Option<BatchStatusReceiver>,
    source: &'a mut JournaldSource,
    cursor: Option<String>,
}

impl<'a> Batch<'a> {
    fn new(source: &'a mut JournaldSource) -> Self {
        let (batch, receiver) = BatchNotifier::maybe_new_with_receiver(source.acknowledgements);
        Self {
            events: Vec::new(),
            record_size: 0,
            exiting: None,
            batch,
            receiver,
            source,
            cursor: None,
        }
    }

    fn handle_next(&mut self, result: Option<Result<Bytes, BoxedFramingError>>) -> bool {
        match result {
            None => {
                warn!("Journalctl process stopped.");
                self.exiting = Some(true);
                false
            }
            Some(Err(error)) => {
                emit!(JournaldReadError { error });
                false
            }
            Some(Ok(bytes)) => {
                match decode_record(&bytes, self.source.remap_priority) {
                    Ok(mut record) => {
                        if self.source.emit_cursor {
                            if let Some(tmp) = record.get(CURSOR) {
                                self.cursor = Some(tmp.clone());
                            }
                        } else if let Some(tmp) = record.remove(CURSOR) {
                            self.cursor = Some(tmp);
                        }

                        if !filter_matches(
                            &record,
                            &self.source.include_matches,
                            &self.source.exclude_matches,
                        ) {
                            self.record_size += bytes.len();

                            let mut event = create_log_event_from_record(
                                record,
                                &self.batch,
                                self.source.log_namespace,
                            );

                            enrich_log_event(&mut event, self.source.log_namespace);

                            self.events.push(event);
                        }
                    }
                    Err(error) => {
                        emit!(JournaldInvalidRecordError {
                            error,
                            text: String::from_utf8_lossy(&bytes).into_owned()
                        });
                    }
                }
                true
            }
        }
    }

    async fn finish(
        mut self,
        finalizer: &Finalizer,
        bytes_received: &'a Registered<BytesReceived>,
        events_received: &'a Registered<EventsReceived>,
    ) -> Option<bool> {
        drop(self.batch);

        if self.record_size > 0 {
            bytes_received.emit(ByteSize(self.record_size));
        }

        if !self.events.is_empty() {
            let count = self.events.len();
            let byte_size = self.events.estimated_json_encoded_size_of();
            events_received.emit(CountByteSize(count, byte_size));

            match self.source.out.send_batch(self.events).await {
                Ok(_) => {
                    if let Some(cursor) = self.cursor {
                        finalizer.finalize(cursor, self.receiver).await;
                    }
                }
                Err(_) => {
                    emit!(StreamClosedError { count });
                    // `out` channel is closed, don't restart journalctl.
                    self.exiting = Some(false);
                }
            }
        }
        self.exiting
    }
}

type JournalStream = BoxStream<'static, Result<Bytes, BoxedFramingError>>;

struct StartJournalctl {
    path: PathBuf,
    systemd_version: u32,
    journal_dir: Option<PathBuf>,
    journal_namespace: Option<String>,
    current_boot_only: bool,
    since_now: bool,
    extra_args: Vec<String>,
}

impl StartJournalctl {
    const fn new(
        path: PathBuf,
        systemd_version: u32,
        journal_dir: Option<PathBuf>,
        journal_namespace: Option<String>,
        current_boot_only: bool,
        since_now: bool,
        extra_args: Vec<String>,
    ) -> Self {
        Self {
            path,
            systemd_version,
            journal_dir,
            journal_namespace,
            current_boot_only,
            since_now,
            extra_args,
        }
    }

    fn make_command(&self, checkpoint: Option<&str>) -> Command {
        let mut command = Command::new(&self.path);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.arg("--follow");
        command.arg("--all");
        command.arg("--show-cursor");
        command.arg("--output=json");

        if let Some(dir) = &self.journal_dir {
            command.arg(format!("--directory={}", dir.display()));
        }

        if let Some(namespace) = &self.journal_namespace {
            command.arg(format!("--namespace={namespace}"));
        }

        // By default entries from all boots are included
        // systemd 242 introduces support for --boot=all
        // systemd 250 lets --follow imply --boot (with no facility to override)
        // systemd 258 allows to override --boot as implied by --follow
        if self.current_boot_only {
            if self.systemd_version < 250 {
                command.arg("--boot");
            }
        } else if self.systemd_version >= 258 {
            command.arg("--boot=all");
        }

        if let Some(cursor) = checkpoint {
            command.arg(format!("--after-cursor={cursor}"));
        } else if self.since_now {
            command.arg("--since=now");
        } else {
            // journalctl --follow only outputs a few lines without a starting point
            command.arg("--since=2000-01-01");
        }

        if !self.extra_args.is_empty() {
            command.args(&self.extra_args);
        }

        command
    }

    fn start(
        &mut self,
        checkpoint: Option<&str>,
    ) -> crate::Result<(JournalStream, JournalStream, RunningJournalctl)> {
        let mut command = self.make_command(checkpoint);

        let mut child = command.spawn().context(JournalctlSpawnSnafu)?;

        let stdout_stream = FramedRead::new(
            child.stdout.take().unwrap(),
            CharacterDelimitedDecoder::new(b'\n'),
        );

        let stderr = child.stderr.take().unwrap();
        let stderr_stream = FramedRead::new(stderr, CharacterDelimitedDecoder::new(b'\n'));

        Ok((
            stdout_stream.boxed(),
            stderr_stream.boxed(),
            RunningJournalctl(child),
        ))
    }
}

struct RunningJournalctl(Child);

impl Drop for RunningJournalctl {
    fn drop(&mut self) {
        if let Some(pid) = self.0.id().and_then(|pid| pid.try_into().ok()) {
            _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
        }
    }
}

async fn get_systemd_version_from_journalctl(journalctl_path: &PathBuf) -> crate::Result<u32> {
    let stdout = Command::new(journalctl_path)
        .arg("--version")
        .output()
        .await
        .context(JournalctlSpawnSnafu)?
        .stdout;

    // output format: `systemd {version_number} ({full_version}){newline}{config ...}`
    let stdout = String::from_utf8_lossy(&stdout);
    Ok(stdout
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| BuildError::JournalctlParseVersion {
            output: {
                let cutoff = 40;
                let length = stdout.chars().count();
                format!(
                    "{}{}",
                    stdout.chars().take(cutoff).collect::<String>(),
                    if length > cutoff {
                        format!(" ..{} more char(s)", length - cutoff)
                    } else {
                        "".to_string()
                    }
                )
            },
        })?)
}

fn enrich_log_event(log: &mut LogEvent, log_namespace: LogNamespace) {
    match log_namespace {
        LogNamespace::Vector => {
            if let Some(host) = log
                .get(metadata_path!(JournaldConfig::NAME, "metadata"))
                .and_then(|meta| meta.get(path!(HOSTNAME)))
            {
                log.insert(metadata_path!(JournaldConfig::NAME, "host"), host.clone());
            }
        }
        LogNamespace::Legacy => {
            if let Some(host) = log.remove(event_path!(HOSTNAME)) {
                log_namespace.insert_source_metadata(
                    JournaldConfig::NAME,
                    log,
                    log_schema().host_key().map(LegacyKey::Overwrite),
                    path!("host"),
                    host,
                );
            }
        }
    }

    // Create a Utc timestamp from an existing log field if present.
    let timestamp_value = match log_namespace {
        LogNamespace::Vector => log
            .get(metadata_path!(JournaldConfig::NAME, "metadata"))
            .and_then(|meta| {
                meta.get(path!(SOURCE_TIMESTAMP))
                    .or_else(|| meta.get(path!(RECEIVED_TIMESTAMP)))
            }),
        LogNamespace::Legacy => log
            .get(event_path!(SOURCE_TIMESTAMP))
            .or_else(|| log.get(event_path!(RECEIVED_TIMESTAMP))),
    };

    let timestamp = timestamp_value
        .filter(|&ts| ts.is_bytes())
        .and_then(|ts| ts.as_str().unwrap().parse::<u64>().ok())
        .map(|ts| {
            chrono::Utc
                .timestamp_opt((ts / 1_000_000) as i64, (ts % 1_000_000) as u32 * 1_000)
                .single()
                .expect("invalid timestamp")
        });

    // Add timestamp.
    match log_namespace {
        LogNamespace::Vector => {
            log.insert(metadata_path!("vector", "ingest_timestamp"), Utc::now());

            if let Some(ts) = timestamp {
                log.insert(metadata_path!(JournaldConfig::NAME, "timestamp"), ts);
            }
        }
        LogNamespace::Legacy => {
            if let Some(ts) = timestamp {
                log.maybe_insert(log_schema().timestamp_key_target_path(), ts);
            }
        }
    }

    // Add source type.
    log_namespace.insert_vector_metadata(
        log,
        log_schema().source_type_key(),
        path!("source_type"),
        JournaldConfig::NAME,
    );
}

fn create_log_event_from_record(
    mut record: Record,
    batch: &Option<BatchNotifier>,
    log_namespace: LogNamespace,
) -> LogEvent {
    match log_namespace {
        LogNamespace::Vector => {
            let message_value = record
                .remove(MESSAGE)
                .map(|msg| Value::Bytes(Bytes::from(msg)))
                .unwrap_or(Value::Null);

            let mut log = LogEvent::from(message_value).with_batch_notifier_option(batch);

            // Add the remaining fields from the Record to the log event into an object to avoid collisions.
            record.iter().for_each(|(key, value)| {
                log.metadata_mut()
                    .value_mut()
                    .insert(path!(JournaldConfig::NAME, "metadata", key), value.as_str());
            });

            log
        }
        LogNamespace::Legacy => {
            let mut log = LogEvent::from_iter(record).with_batch_notifier_option(batch);

            if let Some(message) = log.remove(event_path!(MESSAGE)) {
                log.maybe_insert(log_schema().message_key_target_path(), message);
            }

            log
        }
    }
}

/// Map the given unit name into a valid systemd unit
/// by appending ".service" if no extension is present.
fn fixup_unit(unit: &str) -> String {
    if unit.contains('.') {
        unit.into()
    } else {
        format!("{unit}.service")
    }
}

fn decode_record(line: &[u8], remap: bool) -> Result<Record, JsonError> {
    let mut record = serde_json::from_str::<JsonValue>(&String::from_utf8_lossy(line))?;
    // journalctl will output non-ASCII values using an array
    // of integers. Look for those values and re-parse them.
    if let Some(record) = record.as_object_mut() {
        for (_, value) in record.iter_mut().filter(|(_, v)| v.is_array()) {
            *value = decode_array(value.as_array().expect("already validated"));
        }
    }
    if remap {
        record.get_mut("PRIORITY").map(remap_priority);
    }
    serde_json::from_value(record)
}

fn decode_array(array: &[JsonValue]) -> JsonValue {
    decode_array_as_bytes(array).unwrap_or_else(|| {
        let ser = serde_json::to_string(array).expect("already deserialized");
        JsonValue::String(ser)
    })
}

fn decode_array_as_bytes(array: &[JsonValue]) -> Option<JsonValue> {
    // From the array of values, turn all the numbers into bytes, and
    // then the bytes into a string, but return None if any value in the
    // array was not a valid byte.
    array
        .iter()
        .map(|item| {
            item.as_u64().and_then(|num| match num {
                num if num <= u8::MAX as u64 => Some(num as u8),
                _ => None,
            })
        })
        .collect::<Option<Vec<u8>>>()
        .map(|array| String::from_utf8_lossy(&array).into())
}

fn remap_priority(priority: &mut JsonValue) {
    if let Some(num) = priority.as_str().and_then(|s| usize::from_str(s).ok()) {
        let text = match num {
            0 => "EMERG",
            1 => "ALERT",
            2 => "CRIT",
            3 => "ERR",
            4 => "WARNING",
            5 => "NOTICE",
            6 => "INFO",
            7 => "DEBUG",
            _ => "UNKNOWN",
        };
        *priority = JsonValue::String(text.into());
    }
}

fn filter_matches(record: &Record, includes: &Matches, excludes: &Matches) -> bool {
    match (includes.is_empty(), excludes.is_empty()) {
        (true, true) => false,
        (false, true) => !contains_match(record, includes),
        (true, false) => contains_match(record, excludes),
        (false, false) => !contains_match(record, includes) || contains_match(record, excludes),
    }
}

fn contains_match(record: &Record, matches: &Matches) -> bool {
    let f = move |(field, value)| {
        matches
            .get(field)
            .map(|x| x.contains(value))
            .unwrap_or(false)
    };
    record.iter().any(f)
}

fn find_duplicate_match(a_matches: &Matches, b_matches: &Matches) -> Option<(String, String)> {
    for (a_key, a_values) in a_matches {
        if let Some(b_values) = b_matches.get(a_key.as_str()) {
            for (a, b) in a_values
                .iter()
                .flat_map(|x| std::iter::repeat(x).zip(b_values.iter()))
            {
                if a == b {
                    return Some((a_key.into(), b.into()));
                }
            }
        }
    }
    None
}

enum Finalizer {
    Sync(SharedCheckpointer),
    Async(OrderedFinalizer<String>),
}

impl Finalizer {
    fn new(
        acknowledgements: bool,
        checkpointer: SharedCheckpointer,
        shutdown: ShutdownSignal,
    ) -> Self {
        if acknowledgements {
            let (finalizer, mut ack_stream) = OrderedFinalizer::new(Some(shutdown));
            crate::spawn_in_current_span(async move {
                while let Some((status, cursor)) = ack_stream.next().await {
                    if status == BatchStatus::Delivered {
                        checkpointer.lock().await.set(cursor).await;
                    }
                }
            });
            Self::Async(finalizer)
        } else {
            Self::Sync(checkpointer)
        }
    }

    async fn finalize(&self, cursor: String, receiver: Option<BatchStatusReceiver>) {
        match (self, receiver) {
            (Self::Sync(checkpointer), None) => checkpointer.lock().await.set(cursor).await,
            (Self::Async(finalizer), Some(receiver)) => finalizer.add(cursor, receiver),
            _ => {
                unreachable!("Cannot have async finalization without a receiver in journald source")
            }
        }
    }
}

struct Checkpointer {
    file: File,
    filename: PathBuf,
}

impl Checkpointer {
    async fn new(filename: PathBuf) -> Result<Self, io::Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&filename)
            .await?;
        Ok(Checkpointer { file, filename })
    }

    async fn set(&mut self, token: &str) -> Result<(), io::Error> {
        self.file.seek(SeekFrom::Start(0)).await?;
        self.file.write_all(format!("{token}\n").as_bytes()).await
    }

    async fn get(&mut self) -> Result<Option<String>, io::Error> {
        let mut buf = Vec::<u8>::new();
        self.file.seek(SeekFrom::Start(0)).await?;
        self.file.read_to_end(&mut buf).await?;
        match buf.len() {
            0 => Ok(None),
            _ => {
                let text = String::from_utf8_lossy(&buf);
                Ok(text.split_once('\n').map(|(line, _)| line.to_string()))
            }
        }
    }
}

struct StatefulCheckpointer {
    checkpointer: Checkpointer,
    cursor: Option<String>,
}

impl StatefulCheckpointer {
    async fn new(filename: PathBuf) -> Result<Self, io::Error> {
        let mut checkpointer = Checkpointer::new(filename).await?;
        let cursor = checkpointer.get().await?;
        Ok(Self {
            checkpointer,
            cursor,
        })
    }

    async fn set(&mut self, token: impl Into<String>) {
        let token = token.into();
        if let Err(error) = self.checkpointer.set(&token).await {
            emit!(JournaldCheckpointSetError {
                error,
                filename: self
                    .checkpointer
                    .filename
                    .to_str()
                    .unwrap_or("unknown")
                    .to_string(),
            });
        }
        self.cursor = Some(token);
    }
}

#[derive(Clone)]
struct SharedCheckpointer(Arc<Mutex<StatefulCheckpointer>>);

impl SharedCheckpointer {
    fn new(c: StatefulCheckpointer) -> Self {
        Self(Arc::new(Mutex::new(c)))
    }

    async fn lock(&self) -> MutexGuard<'_, StatefulCheckpointer> {
        self.0.lock().await
    }
}

#[cfg(test)]
mod checkpointer_tests {
    use tempfile::tempdir;
    use tokio::fs::read_to_string;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<JournaldConfig>();
    }

    #[tokio::test]
    async fn journald_checkpointer_works() {
        let tempdir = tempdir().unwrap();
        let mut filename = tempdir.path().to_path_buf();
        filename.push(CHECKPOINT_FILENAME);
        let mut checkpointer = Checkpointer::new(filename.clone())
            .await
            .expect("Creating checkpointer failed!");

        assert!(checkpointer.get().await.unwrap().is_none());

        checkpointer
            .set("first test")
            .await
            .expect("Setting checkpoint failed");
        assert_eq!(checkpointer.get().await.unwrap().unwrap(), "first test");
        let contents = read_to_string(filename.clone())
            .await
            .unwrap_or_else(|_| panic!("Failed to read: {filename:?}"));
        assert!(contents.starts_with("first test\n"));

        checkpointer
            .set("second")
            .await
            .expect("Setting checkpoint failed");
        assert_eq!(checkpointer.get().await.unwrap().unwrap(), "second");
        let contents = read_to_string(filename.clone())
            .await
            .unwrap_or_else(|_| panic!("Failed to read: {filename:?}"));
        assert!(contents.starts_with("second\n"));
    }
}

#[cfg(test)]
mod tests;
