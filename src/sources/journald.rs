use crate::{
    event,
    event::{Event, LogEvent, Value},
    shutdown::ShutdownSignal,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use chrono::TimeZone;
use futures::{
    compat::Future01CompatExt,
    executor::block_on,
    future::{select, Either, FutureExt, TryFutureExt},
};
use futures01::{future, sync::mpsc, Future, Sink};
use lazy_static::lazy_static;
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use serde_json::{Error as JsonError, Value as JsonValue};
use snafu::{ResultExt, Snafu};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::iter::FromIterator;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time;
use string_cache::DefaultAtom as Atom;
use tokio::{task::spawn_blocking, time::delay_for};
use tracing::{dispatcher, field};

const DEFAULT_BATCH_SIZE: usize = 16;

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
}

inventory::submit! {
    SourceDescription::new::<JournaldConfig>("journald")
}

type Record = HashMap<Atom, String>;

#[typetag::serde(name = "journald")]
impl SourceConfig for JournaldConfig {
    fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
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

        let include_units: HashSet<String> = include_units.iter().map(fixup_unit).collect();
        let exclude_units: HashSet<String> = self.exclude_units.iter().map(fixup_unit).collect();
        if let Some(unit) = include_units
            .iter()
            .filter(|unit| exclude_units.contains(&unit[..]))
            .next()
        {
            let unit = unit.into();
            return Err(BuildError::DuplicatedUnit { unit }.into());
        }

        let checkpointer = Checkpointer::new(data_dir)
            .map_err(|err| format!("Unable to open checkpoint file: {}", err))?;

        self.source::<Journalctl>(
            out,
            shutdown,
            checkpointer,
            include_units,
            exclude_units,
            batch_size,
        )
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "journald"
    }
}

impl JournaldConfig {
    fn source<J>(
        &self,
        out: mpsc::Sender<Event>,
        shutdown: ShutdownSignal,
        mut checkpointer: Checkpointer,
        include_units: HashSet<String>,
        exclude_units: HashSet<String>,
        batch_size: usize,
    ) -> crate::Result<super::Source>
    where
        J: JournalSource + Send + 'static,
    {
        let out = out
            .sink_map_err(|_| ())
            .with(|record: Record| future::ok(create_event(record)));

        // Retrieve the saved checkpoint, and use it to seek forward in the journald log
        let cursor = match checkpointer.get() {
            Ok(cursor) => cursor,
            Err(err) => {
                error!(
                    message = "Could not retrieve saved journald checkpoint",
                    error = field::display(&err)
                );
                None
            }
        };

        let (journal, close) = J::new(self, cursor)?;

        Ok(Box::new(future::lazy(move || {
            info!(message = "Starting journald server.",);

            let journald_server = JournaldServer {
                journal,
                include_units,
                exclude_units,
                channel: out,
                shutdown: shutdown.clone(),
                checkpointer,
                batch_size,
            };
            let span = info_span!("journald-server");
            let dispatcher = dispatcher::get_default(|d| d.clone());
            spawn_blocking(move || {
                dispatcher::with_default(&dispatcher, || span.in_scope(|| journald_server.run()))
            })
            .boxed()
            .compat()
            .map_err(|error| error!(message="Journald server unexpectedly stopped.",%error))
            .select(shutdown.map(move |_| close()))
            .map(|_| ())
            .map_err(|_| ())
        })))
    }
}

fn create_event(record: Record) -> Event {
    let mut log = LogEvent::from_iter(record);
    // Convert some journald-specific field names into Vector standard ones.
    if let Some(message) = log.remove(&MESSAGE) {
        log.insert(event::log_schema().message_key().clone(), message);
    }
    if let Some(host) = log.remove(&HOSTNAME) {
        log.insert(event::log_schema().host_key().clone(), host);
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
                log.insert(
                    event::log_schema().timestamp_key().clone(),
                    Value::Timestamp(timestamp),
                );
            }
        }
    }
    // Add source type
    log.try_insert(event::log_schema().source_type_key(), "journald");

    log.into()
}

/// Map the given unit name into a valid systemd unit
/// by appending ".service" if no extension is present.
fn fixup_unit(unit: &String) -> String {
    match unit.contains('.') {
        true => unit.into(),
        false => format!("{}.service", unit),
    }
}

/// A `JournalSource` is a data source that works as an `Iterator`
/// producing lines that resemble journald JSON format records. These
/// trait functions is an addition to the standard iteration methods for
/// initializing the source.
trait JournalSource: Iterator<Item = Result<String, io::Error>> + Sized {
    /// (source, close_underlying_stream)
    fn new(
        config: &JournaldConfig,
        cursor: Option<String>,
    ) -> crate::Result<(Self, Box<dyn FnOnce() + Send>)>;
}

struct Journalctl {
    #[allow(dead_code)]
    child: Child,
    stdout: BufReader<ChildStdout>,
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
        let stdout = child.stdout.take().unwrap();
        let stdout = BufReader::new(stdout);

        let pid = Pid::from_raw(child.id() as i32);
        Ok((
            Journalctl { child, stdout },
            Box::new(move || {
                // Signal the child process to terminate so that the
                // blocking future can be unblocked sooner rather
                // than later.
                let _ = kill(pid, Signal::SIGTERM);
            }),
        ))
    }
}

impl Iterator for Journalctl {
    type Item = Result<String, io::Error>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut line = Vec::<u8>::new();
        match self.stdout.read_until(b'\n', &mut line) {
            Ok(0) => None,
            Ok(_) => Some(Ok(String::from_utf8_lossy(&line).into())),
            Err(err) => Some(Err(err)),
        }
    }
}

struct JournaldServer<J, T> {
    journal: J,
    include_units: HashSet<String>,
    exclude_units: HashSet<String>,
    channel: T,
    shutdown: ShutdownSignal,
    checkpointer: Checkpointer,
    batch_size: usize,
}

impl<J, T> JournaldServer<J, T>
where
    J: JournalSource,
    T: Sink<SinkItem = Record, SinkError = ()>,
{
    pub fn run(mut self) {
        let timeout = time::Duration::from_millis(500); // arbitrary timeout
        let channel = &mut self.channel;
        let mut shutdown = self.shutdown.compat();

        loop {
            let mut saw_record = false;
            let mut at_end = false;
            let mut cursor: Option<String> = None;

            for _ in 0..self.batch_size {
                let text = match self.journal.next() {
                    None => {
                        at_end = true;
                        break;
                    }
                    Some(Ok(text)) => text,
                    Some(Err(err)) => {
                        error!(
                            message = "Could not read from journald source",
                            error = field::display(&err),
                        );
                        break;
                    }
                };

                let mut record = match decode_record(&text) {
                    Ok(record) => record,
                    Err(error) => {
                        error!(message = "Invalid record from journald, discarding", %error, %text);
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

                match channel.send(record).wait() {
                    Ok(_) => {}
                    Err(()) => error!(message = "Could not send journald log"),
                }
            }

            if saw_record {
                if let Some(cursor) = cursor {
                    if let Err(err) = self.checkpointer.set(&cursor) {
                        error!(
                            message = "Could not set journald checkpoint.",
                            error = field::display(&err)
                        );
                    }
                }
            }

            if at_end {
                // This works only if run inside tokio context since we are using
                // tokio's Timer. Outside of such context, this will panic on the first
                // call. Also since we are using block_on here and wait in the above code,
                // this should be run in it's own thread. `spawn_blocking` fulfills
                // all of these requirements.
                match block_on(select(shutdown, delay_for(timeout))) {
                    Either::Left((_, _)) => return,
                    Either::Right((_, future)) => shutdown = future,
                }
            }
        }
    }
}

fn decode_record(text: &str) -> Result<Record, JsonError> {
    let mut record = serde_json::from_str::<JsonValue>(&text)?;
    // journalctl will output non-ASCII messages using an array
    // of integers. Look for those messages and re-parse them.
    record.get_mut("MESSAGE").and_then(|message| {
        message
            .as_array()
            .and_then(decode_array)
            .map(|decoded| *message = decoded)
    });
    serde_json::from_value(record)
}

fn decode_array(array: &Vec<JsonValue>) -> Option<JsonValue> {
    // From the array of values, turn all the numbers into bytes, and
    // then the bytes into a string, but return None if any value in the
    // array was not a valid byte.
    array
        .into_iter()
        .map(|item| {
            item.as_u64().and_then(|num| match num {
                num if num <= u8::max_value() as u64 => Some(num as u8),
                _ => None,
            })
        })
        .collect::<Option<Vec<u8>>>()
        .map(|array| String::from_utf8_lossy(&array).into())
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

const CHECKPOINT_FILENAME: &str = "checkpoint.txt";

struct Checkpointer {
    file: File,
}

impl Checkpointer {
    fn new(mut filename: PathBuf) -> Result<Self, io::Error> {
        filename.push(CHECKPOINT_FILENAME);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(filename)?;
        Ok(Checkpointer { file })
    }

    fn set(&mut self, token: &str) -> Result<(), io::Error> {
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(format!("{}\n", token).as_bytes())?;
        Ok(())
    }

    fn get(&mut self) -> Result<Option<String>, io::Error> {
        let mut buf = Vec::<u8>::new();
        self.file.seek(SeekFrom::Start(0))?;
        self.file.read_to_end(&mut buf)?;
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
    use core::fmt::Debug;
    use std::fs::File;
    use std::io::Read;
    use std::path::Path;
    use tempfile::tempdir;

    fn open_read_close<F: AsRef<Path> + Debug>(path: F) -> Vec<u8> {
        let mut file = File::open(&path).expect(&format!("Could not open {:?}", path));
        let mut buf = Vec::<u8>::new();
        file.read_to_end(&mut buf)
            .expect(&format!("Could not read {:?}", path));
        buf
    }

    #[test]
    fn journald_checkpointer_works() {
        let tempdir = tempdir().unwrap();
        let mut filename = tempdir.path().to_path_buf();
        filename.push(CHECKPOINT_FILENAME);
        let mut checkpointer =
            Checkpointer::new(tempdir.path().to_path_buf()).expect("Creating checkpointer failed!");

        assert!(checkpointer.get().unwrap().is_none());

        checkpointer
            .set("first test")
            .expect("Setting checkpoint failed");
        assert_eq!(checkpointer.get().unwrap().unwrap(), "first test");
        let contents = open_read_close(&filename);
        assert!(String::from_utf8_lossy(&contents).starts_with("first test\n"));

        checkpointer
            .set("second")
            .expect("Setting checkpoint failed");
        assert_eq!(checkpointer.get().unwrap().unwrap(), "second");
        let contents = open_read_close(&filename);
        assert!(String::from_utf8_lossy(&contents).starts_with("second\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{block_on, runtime, shutdown_on_idle};
    use futures01::stream::Stream;
    use std::io::{self, BufReader, Cursor};
    use std::iter::FromIterator;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio01::util::FutureExt;

    const FAKE_JOURNAL: &str = r#"{"_SYSTEMD_UNIT":"sysinit.target","MESSAGE":"System Initialization","__CURSOR":"1","_SOURCE_REALTIME_TIMESTAMP":"1578529839140001"}
{"_SYSTEMD_UNIT":"unit.service","MESSAGE":"unit message","__CURSOR":"2","_SOURCE_REALTIME_TIMESTAMP":"1578529839140002"}
{"_SYSTEMD_UNIT":"badunit.service","MESSAGE":[194,191,72,101,108,108,111,63],"__CURSOR":"2","_SOURCE_REALTIME_TIMESTAMP":"1578529839140003"}
{"_SYSTEMD_UNIT":"stdout","MESSAGE":"Missing timestamp","__CURSOR":"3","__REALTIME_TIMESTAMP":"1578529839140004"}
{"_SYSTEMD_UNIT":"stdout","MESSAGE":"Different timestamps","__CURSOR":"4","_SOURCE_REALTIME_TIMESTAMP":"1578529839140005","__REALTIME_TIMESTAMP":"1578529839140004"}
"#;

    struct FakeJournal {
        reader: BufReader<Cursor<&'static str>>,
    }

    impl Iterator for FakeJournal {
        type Item = Result<String, io::Error>;
        fn next(&mut self) -> Option<Self::Item> {
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

    fn run_journal(iunits: &[&str], xunits: &[&str], cursor: Option<&str>) -> Vec<Event> {
        let (tx, rx) = futures01::sync::mpsc::channel(10);
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();
        let tempdir = tempdir().unwrap();
        let mut checkpointer =
            Checkpointer::new(tempdir.path().to_path_buf()).expect("Creating checkpointer failed!");
        let include_units = HashSet::<String>::from_iter(iunits.into_iter().map(|&s| s.into()));
        let exclude_units = HashSet::<String>::from_iter(xunits.into_iter().map(|&s| s.into()));

        if let Some(cursor) = cursor {
            checkpointer.set(cursor).expect("Could not set checkpoint");
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
            )
            .expect("Creating journald source failed");
        let mut rt = runtime();
        rt.spawn(source);

        std::thread::sleep(Duration::from_millis(100));
        drop(trigger);
        shutdown_on_idle(rt);

        block_on(rx.collect().timeout(Duration::from_secs(1))).expect("Unclosed channel")
    }

    #[test]
    fn reads_journal() {
        let received = run_journal(&[], &[], None);
        assert_eq!(received.len(), 5);
        assert_eq!(
            message(&received[0]),
            Value::Bytes("System Initialization".into())
        );
        assert_eq!(
            received[0].as_log()[event::log_schema().source_type_key()],
            "journald".into()
        );
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140001000));
        assert_eq!(message(&received[1]), Value::Bytes("unit message".into()));
        assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140002000));
    }

    #[test]
    fn includes_units() {
        let received = run_journal(&["unit.service"], &[], None);
        assert_eq!(received.len(), 1);
        assert_eq!(message(&received[0]), Value::Bytes("unit message".into()));
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140002000));
    }

    #[test]
    fn excludes_units() {
        let received = run_journal(&[], &["unit.service", "badunit.service"], None);
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

    #[test]
    fn handles_checkpoint() {
        let received = run_journal(&[], &[], Some("1"));
        assert_eq!(received.len(), 4);
        assert_eq!(message(&received[0]), Value::Bytes("unit message".into()));
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140002000));
    }

    #[test]
    fn parses_array_messages() {
        let received = run_journal(&["badunit.service"], &[], None);
        assert_eq!(received.len(), 1);
        assert_eq!(message(&received[0]), Value::Bytes("¿Hello?".into()));
    }

    #[test]
    fn handles_missing_timestamp() {
        let received = run_journal(&["stdout"], &[], None);
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
        let bar = String::from("bar");
        assert_eq!(filter_unit(Some(&bar), &empty, &empty), false);
        assert_eq!(filter_unit(Some(&bar), &includes, &empty), true);
        assert_eq!(filter_unit(Some(&bar), &empty, &excludes), true);
        assert_eq!(filter_unit(Some(&bar), &includes, &excludes), true);
    }

    fn message(event: &Event) -> Value {
        event.as_log()[&event::log_schema().message_key()].clone()
    }

    fn timestamp(event: &Event) -> Value {
        event.as_log()[&event::log_schema().timestamp_key()].clone()
    }

    fn value_ts(secs: i64, usecs: u32) -> Value {
        Value::Timestamp(chrono::Utc.timestamp(secs, usecs))
    }
}
