use crate::{
    event,
    event::{Event, LogEvent, ValueKind},
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use chrono::TimeZone;
use futures::{future, sync::mpsc, Future, Sink};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::iter::FromIterator;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::thread;
use std::time;
use string_cache::DefaultAtom as Atom;
use tracing::{dispatcher, field};

const DEFAULT_BATCH_SIZE: usize = 16;

lazy_static! {
    static ref CURSOR: Atom = Atom::from("__CURSOR");
    static ref HOSTNAME: Atom = Atom::from("_HOSTNAME");
    static ref MESSAGE: Atom = Atom::from("MESSAGE");
    static ref SYSTEMD_UNIT: Atom = Atom::from("_SYSTEMD_UNIT");
    static ref TIMESTAMP: Atom = Atom::from("_SOURCE_REALTIME_TIMESTAMP");
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("journalctl failed to execute: {}", source))]
    JournalctlSpawn { source: io::Error },
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub struct JournaldConfig {
    pub current_boot_only: Option<bool>,
    pub local_only: Option<bool>,
    pub units: Vec<String>,
    pub data_dir: Option<PathBuf>,
    pub batch_size: Option<usize>,
}

inventory::submit! {
    SourceDescription::new::<JournaldConfig>("journald")
}

#[typetag::serde(name = "journald")]
impl SourceConfig for JournaldConfig {
    fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let local_only = self.local_only.unwrap_or(true);
        let current_boot = self.current_boot_only.unwrap_or(true);
        let data_dir = globals.resolve_and_make_data_subdir(self.data_dir.as_ref(), name)?;
        let batch_size = self.batch_size.unwrap_or(DEFAULT_BATCH_SIZE);

        // Map the given unit names into valid systemd units by
        // appending ".service" if no extension is present.
        let units = self
            .units
            .iter()
            .map(|unit| {
                if unit.contains('.') {
                    unit.into()
                } else {
                    format!("{}.service", unit)
                }
            })
            .collect::<HashSet<String>>();

        let checkpointer = Checkpointer::new(data_dir)
            .map_err(|err| format!("Unable to open checkpoint file: {}", err))?;

        journald_source::<Journalctl>(
            local_only,
            current_boot,
            out,
            checkpointer,
            units,
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

fn journald_source<J>(
    local_only: bool,
    current_boot: bool,
    out: mpsc::Sender<Event>,
    mut checkpointer: Checkpointer,
    units: HashSet<String>,
    batch_size: usize,
) -> crate::Result<super::Source>
where
    J: JournalSource + Send + 'static,
{
    let (shutdown_tx, shutdown_rx) = channel();

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

    let journal = J::new(local_only, current_boot, cursor)?;

    Ok(Box::new(future::lazy(move || {
        info!(message = "Starting journald server.",);

        let journald_server = JournaldServer {
            journal,
            units,
            channel: out,
            shutdown: shutdown_rx,
            checkpointer,
            batch_size,
        };
        let span = info_span!("journald-server");
        let dispatcher = dispatcher::get_default(|d| d.clone());
        thread::spawn(move || {
            dispatcher::with_default(&dispatcher, || span.in_scope(|| journald_server.run()));
        });

        // Dropping shutdown_tx is how we signal to the journald server
        // that it's time to shut down, so it needs to be held onto
        // until the future we return is dropped.
        future::empty().inspect(|_| drop(shutdown_tx))
    })))
}

fn create_event(record: Record) -> Event {
    let mut log = LogEvent::from_iter(record);
    // Convert some journald-specific field names into Vector standard ones.
    if let Some(message) = log.remove(&MESSAGE) {
        log.insert_explicit(event::MESSAGE.clone(), message);
    }
    if let Some(host) = log.remove(&HOSTNAME) {
        log.insert_explicit(event::HOST.clone(), host);
    }
    // Translate the timestamp, and so leave both old and new names.
    if let Some(timestamp) = log.get(&TIMESTAMP) {
        if let ValueKind::Bytes(timestamp) = timestamp {
            if let Ok(timestamp) = String::from_utf8_lossy(timestamp).parse::<u64>() {
                let timestamp = chrono::Utc.timestamp(
                    (timestamp / 1_000_000) as i64,
                    (timestamp % 1_000_000) as u32 * 1_000,
                );
                log.insert_explicit(event::TIMESTAMP.clone(), ValueKind::Timestamp(timestamp));
            }
        }
    }
    log.into()
}

type Record = HashMap<Atom, String>;

trait JournalSource: Iterator<Item = Result<Record, io::Error>> + Sized {
    fn new(local_only: bool, current_boot: bool, cursor: Option<String>) -> crate::Result<Self>;
    fn cursor(&self) -> String;
}

struct Journalctl {
    #[allow(dead_code)]
    child: Child,
    stdout: BufReader<ChildStdout>,
    cursor: String,
}

impl JournalSource for Journalctl {
    fn new(local_only: bool, current_boot: bool, cursor: Option<String>) -> crate::Result<Self> {
        let mut command = Command::new("journalctl");
        command.stdout(Stdio::piped());
        command.arg("--follow");
        command.arg("--all");
        command.arg("--show-cursor");
        command.arg("--output=json");

        if current_boot {
            command.arg("--boot");
        }

        if let Some(cursor) = cursor {
            command.arg(format!("--after-cursor={}", cursor));
        } else {
            // journalctl --follow only outputs a few lines without a starting point
            command.arg("--since=1970-01-01");
        }

        let mut child = command.spawn().context(JournalctlSpawn)?;
        let stdout = child.stdout.take().unwrap();
        let stdout = BufReader::new(stdout);

        Ok(Journalctl {
            child,
            stdout,
            cursor: String::new(),
        })
    }

    fn cursor(&self) -> String {
        self.cursor.clone()
    }
}

impl Iterator for Journalctl {
    type Item = Result<Record, io::Error>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut line = Vec::<u8>::new();
        loop {
            break match self.stdout.read_until(b'\n', &mut line) {
                Ok(0) => None,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&line);
                    let mut record = match serde_json::from_str::<Record>(&text) {
                        Ok(record) => record,
                        Err(error) => {
                            // journalctl will output non-ASCII messages
                            // using an array of integers. We don't
                            // parse them into valid records yet but
                            // instead just skip them.
                            error!(message = "Invalid record from journalctl, discarding", %error, %text);
                            line.clear();
                            continue;
                        }
                    };
                    // The journald format may contain non-string data elements
                    if let Some(cursor) = record.remove(&CURSOR) {
                        self.cursor = cursor;
                    }
                    Some(Ok(record))
                }
                Err(err) => Some(Err(err)),
            };
        }
    }
}

struct JournaldServer<J, T> {
    journal: J,
    units: HashSet<String>,
    channel: T,
    shutdown: Receiver<()>,
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

        loop {
            let mut saw_record = false;
            let mut at_end = false;

            for _ in 0..self.batch_size {
                let record = match self.journal.next() {
                    None => {
                        at_end = true;
                        break;
                    }
                    Some(Ok(record)) => record,
                    Some(Err(err)) => {
                        error!(
                            message = "Could not read from journald source",
                            error = field::display(&err),
                        );
                        break;
                    }
                };
                saw_record = true;
                if !self.units.is_empty() {
                    // Make sure the systemd unit is exactly one of the specified units
                    if let Some(unit) = record.get(&SYSTEMD_UNIT) {
                        if !self.units.contains(unit) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                match channel.send(record).wait() {
                    Ok(_) => {}
                    Err(()) => error!(message = "Could not send journald log"),
                }
            }

            if saw_record {
                let cursor = self.journal.cursor();
                if let Err(err) = self.checkpointer.set(&cursor) {
                    error!(
                        message = "Could not set journald checkpoint.",
                        error = field::display(&err)
                    );
                }
            }

            if at_end {
                match self.shutdown.recv_timeout(timeout) {
                    Ok(()) => unreachable!(), // The sender should never actually send
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
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
    use futures::stream::Stream;
    use std::io;
    use std::iter::FromIterator;
    use std::time::{Duration, SystemTime};
    use stream_cancel::Tripwire;
    use tempfile::tempdir;
    use tokio::util::FutureExt;

    #[derive(Default)]
    struct FakeJournal {
        records: Vec<Record>,
        cursor: usize,
    }

    impl FakeJournal {
        fn push(&mut self, unit: &str, message: &str) {
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("Calculating time stamp failed");
            let mut record = Record::new();
            record.insert("MESSAGE".into(), message.into());
            record.insert("_SYSTEMD_UNIT".into(), unit.into());
            record.insert(
                "_SOURCE_REALTIME_TIMESTAMP".into(),
                format!("{}", timestamp.as_micros()),
            );
            self.records.push(record);
        }
    }

    impl Iterator for FakeJournal {
        type Item = Result<Record, io::Error>;
        fn next(&mut self) -> Option<Self::Item> {
            self.cursor += 1;
            self.records.pop().map(|item| Ok(item))
        }
    }

    impl JournalSource for FakeJournal {
        fn new(_: bool, _: bool, cursor: Option<String>) -> crate::Result<Self> {
            let mut journal = FakeJournal::default();
            journal.push("unit.service", "unit message");
            journal.push("sysinit.target", "System Initialization");

            if let Some(cursor) = cursor {
                let cursor = cursor.parse::<usize>().expect("Invalid cursor");
                for _ in 0..cursor {
                    journal.records.pop();
                }
            }

            Ok(journal)
        }
        // The fake journal cursor is just a line number
        fn cursor(&self) -> String {
            format!("{}", self.cursor)
        }
    }

    fn run_journal(units: &[&str], cursor: Option<&str>) -> Vec<Event> {
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();
        let tempdir = tempdir().unwrap();
        let mut checkpointer =
            Checkpointer::new(tempdir.path().to_path_buf()).expect("Creating checkpointer failed!");
        let units = HashSet::<String>::from_iter(units.into_iter().map(|&s| s.into()));

        if let Some(cursor) = cursor {
            checkpointer.set(cursor).expect("Could not set checkpoint");
        }

        let source = journald_source::<FakeJournal>(
            false,
            false,
            tx,
            checkpointer,
            units,
            DEFAULT_BATCH_SIZE,
        )
        .expect("Creating journald source failed");
        let mut rt = runtime();
        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        std::thread::sleep(Duration::from_millis(100));
        drop(trigger);
        shutdown_on_idle(rt);

        block_on(rx.collect().timeout(Duration::from_secs(1))).expect("Unclosed channel")
    }

    #[test]
    fn journald_source_works() {
        let received = run_journal(&[], None);
        assert_eq!(received.len(), 2);
        assert_eq!(
            received[0].as_log()[&event::MESSAGE],
            ValueKind::Bytes("System Initialization".into())
        );
        assert_eq!(
            received[1].as_log()[&event::MESSAGE],
            ValueKind::Bytes("unit message".into())
        );
    }

    #[test]
    fn journald_source_filters_units() {
        let received = run_journal(&["unit.service"], None);
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log()[&event::MESSAGE],
            ValueKind::Bytes("unit message".into())
        );
    }

    #[test]
    fn journald_source_handles_checkpoint() {
        let received = run_journal(&[], Some("1"));
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log()[&event::MESSAGE],
            ValueKind::Bytes("unit message".into())
        );
    }
}
