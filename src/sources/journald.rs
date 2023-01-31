use std::{
    collections::{HashMap, HashSet},
    io::SeekFrom,
    path::PathBuf,
    process::Stdio,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use codecs::{decoding::BoxedFramingError, CharacterDelimitedDecoder};
use futures::{poll, stream::BoxStream, task::Poll, StreamExt};
use lookup::{lookup_v2::parse_value_path, metadata_path, owned_value_path, path, PathPrefix};
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use once_cell::sync::Lazy;
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
use value::{kind::Collection, Kind, Value};
use vector_common::{
    finalizer::OrderedFinalizer,
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol, Registered,
    },
};
use vector_config::{configurable_component, NamedComponent};
use vector_core::{
    config::{LegacyKey, LogNamespace},
    schema::Definition,
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    config::{
        log_schema, DataType, Output, SourceAcknowledgementsConfig, SourceConfig, SourceContext,
    },
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, LogEvent},
    internal_events::{
        EventsReceived, JournaldCheckpointFileOpenError, JournaldCheckpointSetError,
        JournaldInvalidRecordError, JournaldReadError, JournaldStartJournalctlError,
        StreamClosedError,
    },
    serde::bool_or_struct,
    shutdown::ShutdownSignal,
    SourceSender,
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

static JOURNALCTL: Lazy<PathBuf> = Lazy::new(|| "journalctl".into());

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("journalctl failed to execute: {}", source))]
    JournalctlSpawn { source: io::Error },
    #[snafu(display("Cannot use both `units` and `include_units`"))]
    BothUnitsAndIncludeUnits,
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
}

type Matches = HashMap<String, HashSet<String>>;

/// Configuration for the `journald` source.
#[configurable_component(source("journald"))]
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
    /// Unit names lacking a `.` will have `.service` appended to make them a valid service unit name.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "ntpd", docs::examples = "sysinit.target"))]
    pub include_units: Vec<String>,

    /// A list of unit names to exclude from monitoring.
    ///
    /// Unit names lacking a `.` will have `.service` appended to make them a valid service unit
    /// name.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "badservice", docs::examples = "sysinit.target"))]
    pub exclude_units: Vec<String>,

    /// A list of sets of field/value pairs to monitor.
    ///
    /// If empty or not present, all journal fields are accepted.
    ///
    /// If `include_units` is specified, it will be merged into this list.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "The set of field values to match in journal entries that are to be included."
    ))]
    #[configurable(metadata(docs::examples = "matches_examples()"))]
    pub include_matches: Matches,

    /// A list of sets of field/value pairs that, if any are present in a journal entry, will cause
    /// the entry to be excluded from this source.
    ///
    /// If `exclude_units` is specified, it will be merged into this list.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "The set of field values to match in journal entries that are to be excluded."
    ))]
    #[configurable(metadata(docs::examples = "matches_examples()"))]
    pub exclude_matches: Matches,

    /// The directory used to persist file checkpoint positions.
    ///
    /// By default, the global `data_dir` option is used. Make sure the running user has write
    /// permissions to this directory.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "/var/lib/vector"))]
    pub data_dir: Option<PathBuf>,

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
    /// If not set, `journalctl` will use the default system journal path.
    #[serde(default)]
    pub journal_directory: Option<PathBuf>,

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
}

const fn default_batch_size() -> usize {
    16
}

fn matches_examples() -> HashMap<String, Vec<String>> {
    HashMap::<_, _>::from_iter(
        [
            (
                "_SYSTEMD_UNIT".to_owned(),
                vec!["sshd.service".to_owned(), "ntpd.service".to_owned()],
            ),
            ("_TRANSPORT".to_owned(), vec!["kernel".to_owned()]),
        ]
        .into_iter(),
    )
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
                parse_value_path(log_schema().host_key())
                    .ok()
                    .map(LegacyKey::Overwrite),
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
            acknowledgements: Default::default(),
            remap_priority: false,
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(JournaldConfig);

type Record = HashMap<String, String>;

#[async_trait::async_trait]
impl SourceConfig for JournaldConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        if self.remap_priority {
            warn!("DEPRECATION, option `remap_priority` has been deprecated. Please use the `remap` transform and function `to_syslog_level` instead.");
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

        let starter = StartJournalctl::new(
            journalctl_path,
            self.journal_directory.clone(),
            self.current_boot_only,
            self.since_now,
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
            }
            .run_shutdown(cx.shutdown),
        ))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output> {
        let schema_definition =
            self.schema_definition(global_log_namespace.merge(self.log_namespace));

        vec![Output::default(DataType::Log).with_schema_definition(schema_definition)]
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
                Ok((stream, running)) => {
                    if !self.run_stream(stream, &finalizer, shutdown.clone()).await {
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
        mut stream: JournalStream,
        finalizer: &'a Finalizer,
        mut shutdown: ShutdownSignal,
    ) -> bool {
        let bytes_received = register!(BytesReceived::from(Protocol::from("journald")));
        let events_received = register!(EventsReceived);

        let batch_size = self.batch_size;
        loop {
            let mut batch = Batch::new(self);

            // Start the timeout counter only once we have received a
            // valid and non-filtered event.
            while batch.events.is_empty() {
                let item = tokio::select! {
                    _ = &mut shutdown => return false,
                    item = stream.next() => item,
                };
                if !batch.handle_next(item) {
                    return true;
                }
            }

            let timeout = tokio::time::sleep(BATCH_TIMEOUT);
            tokio::pin!(timeout);

            for _ in 1..batch_size {
                tokio::select! {
                    _ = &mut timeout => break,
                    result = stream.next() => if !batch.handle_next(result) {
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
                        if let Some(tmp) = record.remove(CURSOR) {
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
                Err(error) => {
                    emit!(StreamClosedError { error, count });
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
    journal_dir: Option<PathBuf>,
    current_boot_only: bool,
    since_now: bool,
}

impl StartJournalctl {
    const fn new(
        path: PathBuf,
        journal_dir: Option<PathBuf>,
        current_boot_only: bool,
        since_now: bool,
    ) -> Self {
        Self {
            path,
            journal_dir,
            current_boot_only,
            since_now,
        }
    }

    fn make_command(&self, checkpoint: Option<&str>) -> Command {
        let mut command = Command::new(&self.path);
        command.stdout(Stdio::piped());
        command.arg("--follow");
        command.arg("--all");
        command.arg("--show-cursor");
        command.arg("--output=json");

        if let Some(dir) = &self.journal_dir {
            command.arg(format!("--directory={}", dir.display()));
        }

        if self.current_boot_only {
            command.arg("--boot");
        }

        if let Some(cursor) = checkpoint {
            command.arg(format!("--after-cursor={}", cursor));
        } else if self.since_now {
            command.arg("--since=now");
        } else {
            // journalctl --follow only outputs a few lines without a starting point
            command.arg("--since=2000-01-01");
        }

        command
    }

    fn start(
        &mut self,
        checkpoint: Option<&str>,
    ) -> crate::Result<(JournalStream, RunningJournalctl)> {
        let mut command = self.make_command(checkpoint);

        let mut child = command.spawn().context(JournalctlSpawnSnafu)?;

        let stream = FramedRead::new(
            child.stdout.take().unwrap(),
            CharacterDelimitedDecoder::new(b'\n'),
        )
        .boxed();

        Ok((stream, RunningJournalctl(child)))
    }
}

struct RunningJournalctl(Child);

impl Drop for RunningJournalctl {
    fn drop(&mut self) {
        if let Some(pid) = self.0.id().and_then(|pid| pid.try_into().ok()) {
            let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
        }
    }
}

fn enrich_log_event(log: &mut LogEvent, log_namespace: LogNamespace) {
    if let Some(host) = log.remove(HOSTNAME) {
        log_namespace.insert_source_metadata(
            JournaldConfig::NAME,
            log,
            parse_value_path(log_schema().host_key())
                .ok()
                .as_ref()
                .map(LegacyKey::Overwrite),
            path!("host"),
            host,
        );
    }

    // Create a Utc timestamp from an existing log field if present.
    let timestamp = log
        .get(SOURCE_TIMESTAMP)
        .or_else(|| log.get(RECEIVED_TIMESTAMP))
        .filter(|&ts| ts.is_bytes())
        .and_then(|ts| {
            String::from_utf8_lossy(ts.as_bytes().unwrap())
                .parse::<u64>()
                .ok()
        })
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
                log.insert((PathPrefix::Event, log_schema().timestamp_key()), ts);
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

            if let Some(message) = log.remove(MESSAGE) {
                log.insert(log_schema().message_key(), message);
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
        format!("{}.service", unit)
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
                num if num <= u8::max_value() as u64 => Some(num as u8),
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
            let (finalizer, mut ack_stream) = OrderedFinalizer::new(shutdown);
            tokio::spawn(async move {
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
            .open(&filename)
            .await?;
        Ok(Checkpointer { file, filename })
    }

    async fn set(&mut self, token: &str) -> Result<(), io::Error> {
        self.file.seek(SeekFrom::Start(0)).await?;
        self.file.write_all(format!("{}\n", token).as_bytes()).await
    }

    async fn get(&mut self) -> Result<Option<String>, io::Error> {
        let mut buf = Vec::<u8>::new();
        self.file.seek(SeekFrom::Start(0)).await?;
        self.file.read_to_end(&mut buf).await?;
        match buf.len() {
            0 => Ok(None),
            _ => {
                let text = String::from_utf8_lossy(&buf);
                match text.find('\n') {
                    Some(nl) => Ok(Some(String::from(&text[..nl]))),
                    None => Ok(None), // Maybe return an error?
                }
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
            .unwrap_or_else(|_| panic!("Failed to read: {:?}", filename));
        assert!(contents.starts_with("first test\n"));

        checkpointer
            .set("second")
            .await
            .expect("Setting checkpoint failed");
        assert_eq!(checkpointer.get().await.unwrap().unwrap(), "second");
        let contents = read_to_string(filename.clone())
            .await
            .unwrap_or_else(|_| panic!("Failed to read: {:?}", filename));
        assert!(contents.starts_with("second\n"));
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;
    use tokio::time::{sleep, timeout, Duration, Instant};
    use value::{kind::Collection, Value};

    use super::*;
    use crate::{
        config::ComponentKey, event::Event, event::EventStatus,
        test_util::components::assert_source_compliance,
    };

    const TEST_COMPONENT: &str = "journald-test";
    const TEST_JOURNALCTL: &str = "tests/data/journalctl";

    async fn run_with_units(iunits: &[&str], xunits: &[&str], cursor: Option<&str>) -> Vec<Event> {
        let include_matches = create_unit_matches(iunits.to_vec());
        let exclude_matches = create_unit_matches(xunits.to_vec());
        run_journal(include_matches, exclude_matches, cursor).await
    }

    async fn run_journal(
        include_matches: Matches,
        exclude_matches: Matches,
        checkpoint: Option<&str>,
    ) -> Vec<Event> {
        assert_source_compliance(&["protocol"], async move {
            let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

            let tempdir = tempdir().unwrap();
            let tempdir = tempdir.path().to_path_buf();

            if let Some(cursor) = checkpoint {
                let mut checkpoint_path = tempdir.clone();
                checkpoint_path.push(TEST_COMPONENT);
                fs::create_dir(&checkpoint_path).unwrap();
                checkpoint_path.push(CHECKPOINT_FILENAME);

                let mut checkpointer = Checkpointer::new(checkpoint_path.clone())
                    .await
                    .expect("Creating checkpointer failed!");

                checkpointer
                    .set(cursor)
                    .await
                    .expect("Could not set checkpoint");
            }

            let (cx, shutdown) =
                SourceContext::new_shutdown(&ComponentKey::from(TEST_COMPONENT), tx);
            let config = JournaldConfig {
                journalctl_path: Some(TEST_JOURNALCTL.into()),
                include_matches,
                exclude_matches,
                data_dir: Some(tempdir),
                remap_priority: true,
                acknowledgements: false.into(),
                ..Default::default()
            };
            let source = config.build(cx).await.unwrap();
            tokio::spawn(async move { source.await.unwrap() });

            sleep(Duration::from_millis(100)).await;
            shutdown
                .shutdown_all(Instant::now() + Duration::from_secs(1))
                .await;

            timeout(Duration::from_secs(1), rx.collect()).await.unwrap()
        })
        .await
    }

    fn create_unit_matches<S: Into<String>>(units: Vec<S>) -> Matches {
        let units: HashSet<String> = units.into_iter().map(Into::into).collect();
        let mut map = HashMap::new();
        if !units.is_empty() {
            map.insert(String::from(SYSTEMD_UNIT), units);
        }
        map
    }

    fn create_matches<S: Into<String>>(conditions: Vec<(S, S)>) -> Matches {
        let mut matches: Matches = HashMap::new();
        for (field, value) in conditions {
            matches
                .entry(field.into())
                .or_default()
                .insert(value.into());
        }
        matches
    }

    #[tokio::test]
    async fn reads_journal() {
        let received = run_with_units(&[], &[], None).await;
        assert_eq!(received.len(), 8);
        assert_eq!(
            message(&received[0]),
            Value::Bytes("System Initialization".into())
        );
        assert_eq!(
            received[0].as_log()[log_schema().source_type_key()],
            "journald".into()
        );
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140001000));
        assert_eq!(priority(&received[0]), Value::Bytes("INFO".into()));
        assert_eq!(message(&received[1]), Value::Bytes("unit message".into()));
        assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140002000));
        assert_eq!(priority(&received[1]), Value::Bytes("DEBUG".into()));
    }

    #[tokio::test]
    async fn includes_units() {
        let received = run_with_units(&["unit.service"], &[], None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(message(&received[0]), Value::Bytes("unit message".into()));
    }

    #[tokio::test]
    async fn excludes_units() {
        let received = run_with_units(&[], &["unit.service", "badunit.service"], None).await;
        assert_eq!(received.len(), 6);
        assert_eq!(
            message(&received[0]),
            Value::Bytes("System Initialization".into())
        );
        assert_eq!(
            message(&received[1]),
            Value::Bytes("Missing timestamp".into())
        );
        assert_eq!(
            message(&received[2]),
            Value::Bytes("Different timestamps".into())
        );
    }

    #[tokio::test]
    async fn includes_matches() {
        let matches = create_matches(vec![("PRIORITY", "ERR")]);
        let received = run_journal(matches, HashMap::new(), None).await;
        assert_eq!(received.len(), 2);
        assert_eq!(
            message(&received[0]),
            Value::Bytes("Different timestamps".into())
        );
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140005000));
        assert_eq!(
            message(&received[1]),
            Value::Bytes("Non-ASCII in other field".into())
        );
        assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140005000));
    }

    #[tokio::test]
    async fn includes_kernel() {
        let matches = create_matches(vec![("_TRANSPORT", "kernel")]);
        let received = run_journal(matches, HashMap::new(), None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140006000));
        assert_eq!(message(&received[0]), Value::Bytes("audit log".into()));
    }

    #[tokio::test]
    async fn excludes_matches() {
        let matches = create_matches(vec![("PRIORITY", "INFO"), ("PRIORITY", "DEBUG")]);
        let received = run_journal(HashMap::new(), matches, None).await;
        assert_eq!(received.len(), 5);
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140003000));
        assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140004000));
        assert_eq!(timestamp(&received[2]), value_ts(1578529839, 140005000));
        assert_eq!(timestamp(&received[3]), value_ts(1578529839, 140005000));
        assert_eq!(timestamp(&received[4]), value_ts(1578529839, 140006000));
    }

    #[tokio::test]
    async fn handles_checkpoint() {
        let received = run_with_units(&[], &[], Some("1")).await;
        assert_eq!(received.len(), 7);
        assert_eq!(message(&received[0]), Value::Bytes("unit message".into()));
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140002000));
    }

    #[tokio::test]
    async fn parses_array_messages() {
        let received = run_with_units(&["badunit.service"], &[], None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(message(&received[0]), Value::Bytes("¿Hello?".into()));
    }

    #[tokio::test]
    async fn parses_array_fields() {
        let received = run_with_units(&["syslog.service"], &[], None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log()["SYSLOG_RAW"],
            Value::Bytes("¿World?".into())
        );
    }

    #[tokio::test]
    async fn parses_string_sequences() {
        let received = run_with_units(&["NetworkManager.service"], &[], None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log()["SYSLOG_FACILITY"],
            Value::Bytes(r#"["DHCP4","DHCP6"]"#.into())
        );
    }

    #[tokio::test]
    async fn handles_missing_timestamp() {
        let received = run_with_units(&["stdout"], &[], None).await;
        assert_eq!(received.len(), 2);
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140004000));
        assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140005000));
    }

    #[tokio::test]
    async fn handles_acknowledgements() {
        let (tx, mut rx) = SourceSender::new_test();

        let tempdir = tempdir().unwrap();
        let tempdir = tempdir.path().to_path_buf();
        let mut checkpoint_path = tempdir.clone();
        checkpoint_path.push(TEST_COMPONENT);
        fs::create_dir(&checkpoint_path).unwrap();
        checkpoint_path.push(CHECKPOINT_FILENAME);

        let mut checkpointer = Checkpointer::new(checkpoint_path.clone())
            .await
            .expect("Creating checkpointer failed!");

        let config = JournaldConfig {
            journalctl_path: Some(TEST_JOURNALCTL.into()),
            data_dir: Some(tempdir),
            remap_priority: true,
            acknowledgements: true.into(),
            ..Default::default()
        };
        let (cx, _shutdown) = SourceContext::new_shutdown(&ComponentKey::from(TEST_COMPONENT), tx);
        let source = config.build(cx).await.unwrap();
        tokio::spawn(async move { source.await.unwrap() });

        // Make sure the checkpointer cursor is empty
        assert_eq!(checkpointer.get().await.unwrap(), None);

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Acknowledge all the received events.
        let mut count = 0;
        while let Poll::Ready(Some(event)) = futures::poll!(rx.next()) {
            // The checkpointer shouldn't set the cursor until the end of the batch.
            assert_eq!(checkpointer.get().await.unwrap(), None);
            event.metadata().update_status(EventStatus::Delivered);
            count += 1;
        }
        assert_eq!(count, 8);

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(checkpointer.get().await.unwrap().as_deref(), Some("8"));
    }

    #[test]
    fn filter_matches_works_correctly() {
        let empty: Matches = HashMap::new();
        let includes = create_unit_matches(vec!["one", "two"]);
        let excludes = create_unit_matches(vec!["foo", "bar"]);

        let zero = HashMap::new();
        assert!(!filter_matches(&zero, &empty, &empty));
        assert!(filter_matches(&zero, &includes, &empty));
        assert!(!filter_matches(&zero, &empty, &excludes));
        assert!(filter_matches(&zero, &includes, &excludes));
        let mut one = HashMap::new();
        one.insert(String::from(SYSTEMD_UNIT), String::from("one"));
        assert!(!filter_matches(&one, &empty, &empty));
        assert!(!filter_matches(&one, &includes, &empty));
        assert!(!filter_matches(&one, &empty, &excludes));
        assert!(!filter_matches(&one, &includes, &excludes));
        let mut two = HashMap::new();
        two.insert(String::from(SYSTEMD_UNIT), String::from("bar"));
        assert!(!filter_matches(&two, &empty, &empty));
        assert!(filter_matches(&two, &includes, &empty));
        assert!(filter_matches(&two, &empty, &excludes));
        assert!(filter_matches(&two, &includes, &excludes));
    }

    #[test]
    fn merges_units_and_matches_option() {
        let include_units = vec!["one", "two"].into_iter().map(String::from).collect();
        let include_matches = create_matches(vec![
            ("_SYSTEMD_UNIT", "three.service"),
            ("_TRANSPORT", "kernel"),
        ]);

        let exclude_units = vec!["foo", "bar"].into_iter().map(String::from).collect();
        let exclude_matches = create_matches(vec![
            ("_SYSTEMD_UNIT", "baz.service"),
            ("PRIORITY", "DEBUG"),
        ]);

        let journald_config = JournaldConfig {
            include_units,
            include_matches,
            exclude_units,
            exclude_matches,
            ..Default::default()
        };

        let hashset =
            |v: &[&str]| -> HashSet<String> { v.iter().copied().map(String::from).collect() };

        let matches = journald_config.merged_include_matches();
        let units = matches.get("_SYSTEMD_UNIT").unwrap();
        assert_eq!(
            units,
            &hashset(&["one.service", "two.service", "three.service"])
        );
        let units = matches.get("_TRANSPORT").unwrap();
        assert_eq!(units, &hashset(&["kernel"]));

        let matches = journald_config.merged_exclude_matches();
        let units = matches.get("_SYSTEMD_UNIT").unwrap();
        assert_eq!(
            units,
            &hashset(&["foo.service", "bar.service", "baz.service"])
        );
        let units = matches.get("PRIORITY").unwrap();
        assert_eq!(units, &hashset(&["DEBUG"]));
    }

    #[test]
    fn find_duplicate_match_works_correctly() {
        let include_matches = create_matches(vec![("_TRANSPORT", "kernel")]);
        let exclude_matches = create_matches(vec![("_TRANSPORT", "kernel")]);
        let (field, value) = find_duplicate_match(&include_matches, &exclude_matches).unwrap();
        assert_eq!(field, "_TRANSPORT");
        assert_eq!(value, "kernel");

        let empty = HashMap::new();
        let actual = find_duplicate_match(&empty, &empty);
        assert!(actual.is_none());

        let actual = find_duplicate_match(&include_matches, &empty);
        assert!(actual.is_none());

        let actual = find_duplicate_match(&empty, &exclude_matches);
        assert!(actual.is_none());
    }

    #[test]
    fn command_options() {
        let path = PathBuf::from("journalctl");

        let journal_dir = None;
        let current_boot_only = false;
        let cursor = None;
        let since_now = false;

        let command = create_command(&path, journal_dir, current_boot_only, since_now, cursor);
        let cmd_line = format!("{:?}", command);
        assert!(!cmd_line.contains("--directory="));
        assert!(!cmd_line.contains("--boot"));
        assert!(cmd_line.contains("--since=2000-01-01"));

        let since_now = true;
        let journal_dir = None;

        let command = create_command(&path, journal_dir, current_boot_only, since_now, cursor);
        let cmd_line = format!("{:?}", command);
        assert!(cmd_line.contains("--since=now"));

        let journal_dir = Some(PathBuf::from("/tmp/journal-dir"));
        let current_boot_only = true;
        let cursor = Some("2021-01-01");

        let command = create_command(&path, journal_dir, current_boot_only, since_now, cursor);
        let cmd_line = format!("{:?}", command);
        assert!(cmd_line.contains("--directory=/tmp/journal-dir"));
        assert!(cmd_line.contains("--boot"));
        assert!(cmd_line.contains("--after-cursor="));
    }

    fn create_command(
        path: &Path,
        journal_dir: Option<PathBuf>,
        current_boot_only: bool,
        since_now: bool,
        cursor: Option<&str>,
    ) -> Command {
        StartJournalctl::new(path.into(), journal_dir, current_boot_only, since_now)
            .make_command(cursor)
    }

    fn message(event: &Event) -> Value {
        event.as_log()[log_schema().message_key()].clone()
    }

    fn timestamp(event: &Event) -> Value {
        event.as_log()[log_schema().timestamp_key()].clone()
    }

    fn value_ts(secs: i64, usecs: u32) -> Value {
        Value::Timestamp(
            chrono::Utc
                .timestamp_opt(secs, usecs)
                .single()
                .expect("invalid timestamp"),
        )
    }

    fn priority(event: &Event) -> Value {
        event.as_log()["PRIORITY"].clone()
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = JournaldConfig {
            log_namespace: Some(true),
            ..Default::default()
        };

        let definition = config.outputs(LogNamespace::Vector)[0]
            .clone()
            .log_schema_definition
            .unwrap();

        let expected_definition =
            Definition::new_with_default_metadata(Kind::bytes().or_null(), [LogNamespace::Vector])
                .with_metadata_field(&owned_value_path!("vector", "source_type"), Kind::bytes())
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                )
                .with_metadata_field(
                    &owned_value_path!(JournaldConfig::NAME, "metadata"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                )
                .with_metadata_field(
                    &owned_value_path!(JournaldConfig::NAME, "timestamp"),
                    Kind::timestamp().or_undefined(),
                )
                .with_metadata_field(
                    &owned_value_path!(JournaldConfig::NAME, "host"),
                    Kind::bytes().or_undefined(),
                );

        assert_eq!(definition, expected_definition)
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = JournaldConfig::default();

        let definition = config.outputs(LogNamespace::Legacy)[0]
            .clone()
            .log_schema_definition
            .unwrap();

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
        .with_event_field(
            &owned_value_path!("host"),
            Kind::bytes().or_undefined(),
            Some("host"),
        )
        .unknown_fields(Kind::bytes());

        assert_eq!(definition, expected_definition)
    }

    fn matches_schema(config: &JournaldConfig, namespace: LogNamespace) {
        let record = r#"{
            "PRIORITY":"6",
            "SYSLOG_FACILITY":"3",
            "SYSLOG_IDENTIFIER":"ntpd",
            "_BOOT_ID":"124c781146e841ae8d9b4590df8b9231",
            "_CAP_EFFECTIVE":"3fffffffff",
            "_CMDLINE":"ntpd: [priv]",
            "_COMM":"ntpd",
            "_EXE":"/usr/sbin/ntpd",
            "_GID":"0",
            "_MACHINE_ID":"c36e9ea52800a19d214cb71b53263a28",
            "_PID":"2156",
            "_STREAM_ID":"92c79f4b45c4457490ebdefece29995e",
            "_SYSTEMD_CGROUP":"/system.slice/ntpd.service",
            "_SYSTEMD_INVOCATION_ID":"496ad5cd046d48e29f37f559a6d176f8",
            "_SYSTEMD_SLICE":"system.slice",
            "_SYSTEMD_UNIT":"ntpd.service",
            "_TRANSPORT":"stdout",
            "_UID":"0",
            "__MONOTONIC_TIMESTAMP":"98694000446",
            "__REALTIME_TIMESTAMP":"1564173027000443",
            "host":"my-host.local",
            "message":"reply from 192.168.1.2: offset -0.001791 delay 0.000176, next query 1500s",
            "source_type":"journald"
        }"#;

        let json: serde_json::Value = serde_json::from_str(record).unwrap();
        let mut event = Event::from(LogEvent::from(value::Value::from(json)));

        event.as_mut_log().insert("timestamp", chrono::Utc::now());

        let definition = config.outputs(namespace)[0]
            .clone()
            .log_schema_definition
            .unwrap();

        definition.assert_valid_for_event(&event)
    }

    #[test]
    fn matches_schema_legacy() {
        let config = JournaldConfig::default();

        matches_schema(&config, LogNamespace::Legacy)
    }
}
