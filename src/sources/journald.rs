use crate::{
    config::{log_schema, DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::{Event, LogEvent, Value},
    internal_events::{JournaldEventReceived, JournaldInvalidRecord},
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use chrono::TimeZone;
use futures::{compat::Future01CompatExt, ready, FutureExt, Stream, StreamExt, TryFutureExt};
use futures01::{future, Future, Sink};
use lazy_static::lazy_static;
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use serde_json::{Error as JsonError, Value as JsonValue};
use snafu::{ResultExt, Snafu};
use std::{
    collections::{HashMap, HashSet},
    io::SeekFrom,
    iter::FromIterator,
    path::PathBuf,
    pin::Pin,
    process::Stdio,
    str::FromStr,
    task::{Context, Poll},
};
use string_cache::DefaultAtom as Atom;
use tokio::{
    fs::{File, OpenOptions},
    io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdout, Command},
};
use tracing_futures::Instrument;

const DEFAULT_BATCH_SIZE: usize = 16;

const CHECKPOINT_FILENAME: &str = "checkpoint.txt";

lazy_static! {
    static ref CURSOR: Atom = Atom::from("__CURSOR");
    static ref HOSTNAME: Atom = Atom::from("_HOSTNAME");
    static ref MESSAGE: Atom = Atom::from("MESSAGE");
    static ref SYSTEMD_UNIT: Atom = Atom::from("_SYSTEMD_UNIT");
    static ref SOURCE_TIMESTAMP: Atom = Atom::from("_SOURCE_REALTIME_TIMESTAMP");
    static ref RECEIVED_TIMESTAMP: Atom = Atom::from("__REALTIME_TIMESTAMP");
    static ref JOURNALCTL: PathBuf = "journalctl".into();
}

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
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub struct JournaldConfig {
    pub current_boot_only: Option<bool>,
    pub units: Vec<String>,
    pub include_units: Vec<String>,
    pub exclude_units: Vec<String>,
    pub data_dir: Option<PathBuf>,
    pub batch_size: Option<usize>,
    pub journalctl_path: Option<PathBuf>,
    #[serde(default)]
    pub remap_priority: bool,
}

inventory::submit! {
    SourceDescription::new::<JournaldConfig>("journald")
}

impl_generate_config_from_default!(JournaldConfig);

type Record = HashMap<Atom, String>;

#[async_trait::async_trait]
#[typetag::serde(name = "journald")]
impl SourceConfig for JournaldConfig {
    async fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let data_dir = globals.resolve_and_make_data_subdir(self.data_dir.as_ref(), name)?;
        let batch_size = self.batch_size.unwrap_or(DEFAULT_BATCH_SIZE);

        let include_units = match (!self.units.is_empty(), !self.include_units.is_empty()) {
            (true, true) => return Err(BuildError::BothUnitsAndIncludeUnits.into()),
            (true, false) => {
                warn!("The `units` setting is deprecated, use `include_units` instead");
                &self.units
            }
            (false, _) => &self.include_units,
        };

        let include_units: HashSet<String> = include_units.iter().map(|s| fixup_unit(&s)).collect();
        let exclude_units: HashSet<String> =
            self.exclude_units.iter().map(|s| fixup_unit(&s)).collect();
        if let Some(unit) = include_units
            .iter()
            .find(|unit| exclude_units.contains(&unit[..]))
        {
            let unit = unit.into();
            return Err(BuildError::DuplicatedUnit { unit }.into());
        }

        let mut checkpoint = data_dir;
        checkpoint.push(CHECKPOINT_FILENAME);
        let checkpointer = Checkpointer::new(checkpoint.clone())
            .await
            .map_err(|error| {
                format!("Unable to open checkpoint file {:?}: {}", checkpoint, error)
            })?;

        self.source::<Journalctl>(
            out,
            shutdown,
            checkpointer,
            include_units,
            exclude_units,
            batch_size,
            self.remap_priority,
        )
        .await
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "journald"
    }
}

impl JournaldConfig {
    async fn source<J>(
        &self,
        out: Pipeline,
        shutdown: ShutdownSignal,
        mut checkpointer: Checkpointer,
        include_units: HashSet<String>,
        exclude_units: HashSet<String>,
        batch_size: usize,
        remap_priority: bool,
    ) -> crate::Result<super::Source>
    where
        J: JournalSource + Send + 'static,
    {
        let out = out
            .sink_map_err(|_| ())
            .with(|record: Record| future::ok(create_event(record)));

        // Retrieve the saved checkpoint, and use it to seek forward in the journald log
        let cursor = match checkpointer.get().await {
            Ok(cursor) => cursor,
            Err(err) => {
                error!(
                    message = "Could not retrieve saved journald checkpoint",
                    error = %err
                );
                None
            }
        };

        let (journal, close) = J::new(self, cursor)?;
        let journald_server = JournaldServer {
            journal: Box::pin(journal),
            include_units,
            exclude_units,
            channel: out,
            shutdown: shutdown.clone(),
            checkpointer,
            batch_size,
            remap_priority,
        };

        Ok(Box::new(
            async move {
                info!(message = "Starting journald server.",);
                journald_server.run().await;
                Ok(())
            }
            .instrument(info_span!("journald-server"))
            .boxed()
            .compat()
            .select(shutdown.map(move |_| close()))
            .map(|_| ())
            .map_err(|_| ()),
        ))
    }
}

fn create_event(record: Record) -> Event {
    let mut log = LogEvent::from_iter(record);
    // Convert some journald-specific field names into Vector standard ones.
    if let Some(message) = log.remove(&MESSAGE) {
        log.insert(log_schema().message_key(), message);
    }
    if let Some(host) = log.remove(&HOSTNAME) {
        log.insert(log_schema().host_key(), host);
    }
    // Translate the timestamp, and so leave both old and new names.
    if let Some(timestamp) = log
        .get(&SOURCE_TIMESTAMP)
        .or_else(|| log.get(&RECEIVED_TIMESTAMP))
    {
        if let Value::Bytes(timestamp) = timestamp {
            if let Ok(timestamp) = String::from_utf8_lossy(timestamp).parse::<u64>() {
                let timestamp = chrono::Utc.timestamp(
                    (timestamp / 1_000_000) as i64,
                    (timestamp % 1_000_000) as u32 * 1_000,
                );
                log.insert(log_schema().timestamp_key(), Value::Timestamp(timestamp));
            }
        }
    }
    // Add source type
    log.try_insert(
        &Atom::from(log_schema().source_type_key()),
        Bytes::from("journald"),
    );

    log.into()
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

/// A `JournalSource` is a data source that works as a `Stream`
/// producing lines that resemble journald JSON format records. These
/// trait functions is an addition to the standard stream methods for
/// initializing the source.
trait JournalSource: Stream<Item = io::Result<String>> + Sized {
    /// (source, close_underlying_stream)
    fn new(
        config: &JournaldConfig,
        cursor: Option<String>,
    ) -> crate::Result<(Self, Box<dyn FnOnce() + Send>)>;
}

#[pin_project::pin_project]
struct Journalctl {
    #[pin]
    inner: io::Split<BufReader<ChildStdout>>,
}

impl JournalSource for Journalctl {
    fn new(
        config: &JournaldConfig,
        cursor: Option<String>,
    ) -> crate::Result<(Self, Box<dyn FnOnce() + Send>)> {
        let journalctl = config.journalctl_path.as_ref().unwrap_or(&JOURNALCTL);
        let mut command = Command::new(journalctl);
        command.stdout(Stdio::piped());
        command.arg("--follow");
        command.arg("--all");
        command.arg("--show-cursor");
        command.arg("--output=json");

        let current_boot = config.current_boot_only.unwrap_or(true);
        if current_boot {
            command.arg("--boot");
        }

        if let Some(cursor) = cursor {
            command.arg(format!("--after-cursor={}", cursor));
        } else {
            // journalctl --follow only outputs a few lines without a starting point
            command.arg("--since=2000-01-01");
        }

        let mut child = command.spawn().context(JournalctlSpawn)?;
        let stdout = BufReader::new(child.stdout.take().unwrap());

        let pid = Pid::from_raw(child.id() as i32);
        Ok((
            Journalctl {
                inner: stdout.split(b'\n'),
            },
            Box::new(move || {
                // Signal the child process to terminate so that the
                // blocking future can be unblocked sooner rather
                // than later.
                let _ = kill(pid, Signal::SIGTERM);
            }),
        ))
    }
}

impl Stream for Journalctl {
    type Item = io::Result<String>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        Poll::Ready(match ready!(self.project().inner.poll_next(cx)) {
            Some(Ok(segment)) => Some(Ok(String::from_utf8_lossy(&segment).into())),
            Some(Err(err)) => Some(Err(err)),
            None => None,
        })
    }
}

struct JournaldServer<J, T> {
    journal: Pin<Box<J>>,
    include_units: HashSet<String>,
    exclude_units: HashSet<String>,
    channel: T,
    shutdown: ShutdownSignal,
    checkpointer: Checkpointer,
    batch_size: usize,
    remap_priority: bool,
}

impl<J, T> JournaldServer<J, T>
where
    J: JournalSource,
    T: Sink<SinkItem = Record, SinkError = ()>,
{
    pub async fn run(mut self) {
        let channel = &mut self.channel;

        loop {
            let mut saw_record = false;
            let mut cursor: Option<String> = None;

            for _ in 0..self.batch_size {
                let text = match self.journal.next().await {
                    None => {
                        let _ = self.shutdown.compat().await;
                        return;
                    }
                    Some(Ok(text)) => text,
                    Some(Err(err)) => {
                        error!(
                            message = "Could not read from journald source",
                            error = %err,
                        );
                        break;
                    }
                };

                let mut record = match decode_record(&text, self.remap_priority) {
                    Ok(record) => record,
                    Err(error) => {
                        emit!(JournaldInvalidRecord { error, text });
                        continue;
                    }
                };
                if let Some(tmp) = record.remove(&CURSOR) {
                    cursor = Some(tmp);
                }

                saw_record = true;

                let unit = record.get(&SYSTEMD_UNIT);
                if filter_unit(unit, &self.include_units, &self.exclude_units) {
                    continue;
                }

                emit!(JournaldEventReceived {
                    byte_size: text.len()
                });

                match channel.send(record).compat().await {
                    Ok(_) => {}
                    Err(()) => error!(message = "Could not send journald log"),
                }
            }

            if saw_record {
                if let Some(cursor) = cursor {
                    if let Err(error) = self.checkpointer.set(&cursor).await {
                        error!(
                            message = "Could not set journald checkpoint.",
                            %error,
                            filename = ?self.checkpointer.filename,
                        );
                    }
                }
            }
        }
    }
}

fn decode_record(text: &str, remap: bool) -> Result<Record, JsonError> {
    let mut record = serde_json::from_str::<JsonValue>(&text)?;
    // journalctl will output non-ASCII messages using an array
    // of integers. Look for those messages and re-parse them.
    record.get_mut("MESSAGE").and_then(|message| {
        message
            .as_array()
            .and_then(|v| decode_array(&v))
            .map(|decoded| *message = decoded)
    });
    if remap {
        record.get_mut("PRIORITY").map(remap_priority);
    }
    serde_json::from_value(record)
}

fn decode_array(array: &[JsonValue]) -> Option<JsonValue> {
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

/// Should the given unit name be filtered (excluded)?
fn filter_unit(
    unit: Option<&String>,
    includes: &HashSet<String>,
    excludes: &HashSet<String>,
) -> bool {
    match (unit, includes.is_empty(), excludes.is_empty()) {
        (None, empty, _) => !empty,
        (Some(_), true, true) => false,
        (Some(unit), false, true) => !includes.contains(unit),
        (Some(unit), true, false) => excludes.contains(unit),
        (Some(unit), false, false) => !includes.contains(unit) || excludes.contains(unit),
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
        self.file
            .write_all(format!("{}\n", token).as_bytes())
            .await?;
        Ok(())
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

#[cfg(test)]
mod checkpointer_tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs::read_to_string;

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
    use super::*;
    use crate::Pipeline;
    use futures01::stream::Stream as _;
    use std::{
        io::{BufRead, BufReader, Cursor},
        iter::FromIterator,
    };
    use tempfile::tempdir;
    use tokio::{
        io,
        time::{delay_for, timeout, Duration},
    };

    const FAKE_JOURNAL: &str = r#"{"_SYSTEMD_UNIT":"sysinit.target","MESSAGE":"System Initialization","__CURSOR":"1","_SOURCE_REALTIME_TIMESTAMP":"1578529839140001","PRIORITY":"6"}
{"_SYSTEMD_UNIT":"unit.service","MESSAGE":"unit message","__CURSOR":"2","_SOURCE_REALTIME_TIMESTAMP":"1578529839140002","PRIORITY":"7"}
{"_SYSTEMD_UNIT":"badunit.service","MESSAGE":[194,191,72,101,108,108,111,63],"__CURSOR":"2","_SOURCE_REALTIME_TIMESTAMP":"1578529839140003","PRIORITY":"5"}
{"_SYSTEMD_UNIT":"stdout","MESSAGE":"Missing timestamp","__CURSOR":"3","__REALTIME_TIMESTAMP":"1578529839140004","PRIORITY":"2"}
{"_SYSTEMD_UNIT":"stdout","MESSAGE":"Different timestamps","__CURSOR":"4","_SOURCE_REALTIME_TIMESTAMP":"1578529839140005","__REALTIME_TIMESTAMP":"1578529839140004","PRIORITY":"3"}
"#;

    struct FakeJournal {
        reader: BufReader<Cursor<&'static str>>,
    }

    impl FakeJournal {
        fn next(&mut self) -> Option<io::Result<String>> {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => None,
                Ok(_) => {
                    line.pop();
                    Some(Ok(line))
                }
                Err(err) => Some(Err(err)),
            }
        }
    }

    impl Stream for FakeJournal {
        type Item = io::Result<String>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<Self::Item>> {
            Poll::Ready(Pin::into_inner(self).next())
        }
    }

    impl JournalSource for FakeJournal {
        fn new(
            _: &JournaldConfig,
            checkpoint: Option<String>,
        ) -> crate::Result<(Self, Box<dyn FnOnce() + Send>)> {
            let cursor = Cursor::new(FAKE_JOURNAL);
            let reader = BufReader::new(cursor);
            let mut journal = FakeJournal { reader };

            // The cursors are simply line numbers
            if let Some(cursor) = checkpoint {
                let cursor = cursor.parse::<usize>().expect("Invalid cursor");
                for _ in 0..cursor {
                    journal.next();
                }
            }

            Ok((journal, Box::new(|| ())))
        }
    }

    async fn run_journal(iunits: &[&str], xunits: &[&str], cursor: Option<&str>) -> Vec<Event> {
        let (tx, rx) = Pipeline::new_test();
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();
        let tempdir = tempdir().unwrap();
        let mut filename = tempdir.path().to_path_buf();
        filename.push(CHECKPOINT_FILENAME);
        let mut checkpointer = Checkpointer::new(filename)
            .await
            .expect("Creating checkpointer failed!");
        let include_units = HashSet::<String>::from_iter(iunits.iter().map(|&s| s.into()));
        let exclude_units = HashSet::<String>::from_iter(xunits.iter().map(|&s| s.into()));

        if let Some(cursor) = cursor {
            checkpointer
                .set(cursor)
                .await
                .expect("Could not set checkpoint");
        }

        let config = JournaldConfig::default();
        let source = config
            .source::<FakeJournal>(
                tx,
                shutdown,
                checkpointer,
                include_units,
                exclude_units,
                DEFAULT_BATCH_SIZE,
                true,
            )
            .await
            .expect("Creating journald source failed");
        tokio::spawn(source.compat());

        delay_for(Duration::from_millis(100)).await;
        drop(trigger);

        timeout(Duration::from_secs(1), rx.collect().compat())
            .await
            .expect("Unclosed channel")
            .unwrap()
    }

    #[tokio::test]
    async fn reads_journal() {
        let received = run_journal(&[], &[], None).await;
        assert_eq!(received.len(), 5);
        assert_eq!(
            message(&received[0]),
            Value::Bytes("System Initialization".into())
        );
        assert_eq!(
            received[0].as_log()[&Atom::from(log_schema().source_type_key())],
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
        let received = run_journal(&["unit.service"], &[], None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(message(&received[0]), Value::Bytes("unit message".into()));
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140002000));
    }

    #[tokio::test]
    async fn excludes_units() {
        let received = run_journal(&[], &["unit.service", "badunit.service"], None).await;
        assert_eq!(received.len(), 3);
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
    async fn handles_checkpoint() {
        let received = run_journal(&[], &[], Some("1")).await;
        assert_eq!(received.len(), 4);
        assert_eq!(message(&received[0]), Value::Bytes("unit message".into()));
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140002000));
    }

    #[tokio::test]
    async fn parses_array_messages() {
        let received = run_journal(&["badunit.service"], &[], None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(message(&received[0]), Value::Bytes("¿Hello?".into()));
    }

    #[tokio::test]
    async fn handles_missing_timestamp() {
        let received = run_journal(&["stdout"], &[], None).await;
        assert_eq!(received.len(), 2);
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140004000));
        assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140005000));
    }

    #[test]
    fn filter_unit_works_correctly() {
        let empty: HashSet<String> = vec![].into_iter().collect();
        let includes: HashSet<String> = vec!["one", "two"].into_iter().map(Into::into).collect();
        let excludes: HashSet<String> = vec!["foo", "bar"].into_iter().map(Into::into).collect();

        assert_eq!(filter_unit(None, &empty, &empty), false);
        assert_eq!(filter_unit(None, &includes, &empty), true);
        assert_eq!(filter_unit(None, &empty, &excludes), false);
        assert_eq!(filter_unit(None, &includes, &excludes), true);
        let one = String::from("one");
        assert_eq!(filter_unit(Some(&one), &empty, &empty), false);
        assert_eq!(filter_unit(Some(&one), &includes, &empty), false);
        assert_eq!(filter_unit(Some(&one), &empty, &excludes), false);
        assert_eq!(filter_unit(Some(&one), &includes, &excludes), false);
        let two = String::from("bar");
        assert_eq!(filter_unit(Some(&two), &empty, &empty), false);
        assert_eq!(filter_unit(Some(&two), &includes, &empty), true);
        assert_eq!(filter_unit(Some(&two), &empty, &excludes), true);
        assert_eq!(filter_unit(Some(&two), &includes, &excludes), true);
    }

    fn message(event: &Event) -> Value {
        event.as_log()[&Atom::from(log_schema().message_key())].clone()
    }

    fn timestamp(event: &Event) -> Value {
        event.as_log()[&Atom::from(log_schema().timestamp_key())].clone()
    }

    fn value_ts(secs: i64, usecs: u32) -> Value {
        Value::Timestamp(chrono::Utc.timestamp(secs, usecs))
    }

    fn priority(event: &Event) -> Value {
        event.as_log()[&"PRIORITY".into()].clone()
    }
}
