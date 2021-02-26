use crate::{
    config::{log_schema, DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::{Event, LogEvent, Value},
    internal_events::{JournaldEventReceived, JournaldInvalidRecord},
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use chrono::TimeZone;
use codec::BytesDelimitedCodec;
use futures::{future, stream::BoxStream, SinkExt, StreamExt};
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
    process::Stdio,
    str::FromStr,
    time::Duration,
};
use tokio_util::codec::FramedRead;

use tokio::{
    fs::{File, OpenOptions},
    io::{self, AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::delay_for,
};
use tracing_futures::Instrument;

const DEFAULT_BATCH_SIZE: usize = 16;

const CHECKPOINT_FILENAME: &str = "checkpoint.txt";
const CURSOR: &str = "__CURSOR";
const HOSTNAME: &str = "_HOSTNAME";
const MESSAGE: &str = "MESSAGE";
const SYSTEMD_UNIT: &str = "_SYSTEMD_UNIT";
const SOURCE_TIMESTAMP: &str = "_SOURCE_REALTIME_TIMESTAMP";
const RECEIVED_TIMESTAMP: &str = "__REALTIME_TIMESTAMP";

const BACKOFF_DURATION: Duration = Duration::from_secs(1);

lazy_static! {
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
    /// Deprecated
    #[serde(default)]
    remap_priority: bool,
}

inventory::submit! {
    SourceDescription::new::<JournaldConfig>("journald")
}

impl_generate_config_from_default!(JournaldConfig);

type Record = HashMap<String, String>;

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
        if self.remap_priority {
            warn!("Option `remap_priority` has been deprecated. Please use the `remap` transform and function `to_syslog_level` instead.");
        }

        let data_dir = globals.resolve_and_make_data_subdir(self.data_dir.as_ref(), name)?;

        let include_units = match (!self.units.is_empty(), !self.include_units.is_empty()) {
            (true, true) => return Err(BuildError::BothUnitsAndIncludeUnits.into()),
            (true, false) => {
                warn!("The `units` setting is deprecated, use `include_units` instead.");
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

        let mut checkpoint_path = data_dir;
        checkpoint_path.push(CHECKPOINT_FILENAME);

        let journalctl_path = self
            .journalctl_path
            .clone()
            .unwrap_or_else(|| JOURNALCTL.clone());

        let batch_size = self.batch_size.unwrap_or(DEFAULT_BATCH_SIZE);
        let current_boot_only = self.current_boot_only.unwrap_or(true);

        let start: StartJournalctlFn =
            Box::new(move |cursor| start_journalctl(&journalctl_path, current_boot_only, cursor));

        Ok(Box::pin(
            JournaldSource {
                include_units,
                exclude_units,
                checkpoint_path,
                batch_size,
                remap_priority: self.remap_priority,
                out,
            }
            .run_shutdown(shutdown, start)
            .instrument(info_span!("journald-server")),
        ))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "journald"
    }
}

struct JournaldSource {
    include_units: HashSet<String>,
    exclude_units: HashSet<String>,
    checkpoint_path: PathBuf,
    batch_size: usize,
    remap_priority: bool,
    out: Pipeline,
}

impl JournaldSource {
    async fn run_shutdown(
        self,
        shutdown: ShutdownSignal,
        start_journalctl: StartJournalctlFn,
    ) -> Result<(), ()> {
        let mut checkpointer = Checkpointer::new(self.checkpoint_path.clone())
            .await
            .map_err(|error| {
                error!(
                    message = "Unable to open checkpoint file.",
                    path = ?self.checkpoint_path,
                    %error,
                );
            })?;

        let mut cursor = match checkpointer.get().await {
            Ok(cursor) => cursor,
            Err(error) => {
                error!(
                    message = "Could not retrieve saved journald checkpoint.",
                    %error
                );
                None
            }
        };

        let mut on_stop = None;
        let run = Box::pin(self.run(
            &mut checkpointer,
            &mut cursor,
            &mut on_stop,
            start_journalctl,
        ));
        future::select(run, shutdown).await;

        if let Some(stop) = on_stop {
            stop();
        }

        Self::save_checkpoint(&mut checkpointer, &cursor).await;

        Ok(())
    }

    async fn run<'a>(
        mut self,
        checkpointer: &'a mut Checkpointer,
        cursor: &'a mut Option<String>,
        on_stop: &'a mut Option<StopJournalctlFn>,
        start_journalctl: StartJournalctlFn,
    ) {
        loop {
            info!("Starting journalctl.");
            match start_journalctl(&*cursor) {
                Ok((stream, stop)) => {
                    *on_stop = Some(stop);
                    let should_restart = self.run_stream(stream, checkpointer, cursor).await;
                    if let Some(stop) = on_stop.take() {
                        stop();
                    }
                    if !should_restart {
                        return;
                    }
                }
                Err(error) => {
                    error!(message = "Error starting journalctl process.", %error);
                }
            };

            // journalctl process should never stop,
            // so it is an error if we reach here.
            delay_for(BACKOFF_DURATION).await;
        }
    }

    /// Process `journalctl` output until some error occurs.
    /// Return `true` if should restart `journalctl`.
    async fn run_stream<'a>(
        &'a mut self,
        mut stream: BoxStream<'static, io::Result<Bytes>>,
        checkpointer: &'a mut Checkpointer,
        cursor: &'a mut Option<String>,
    ) -> bool {
        loop {
            let mut saw_record = false;

            for _ in 0..self.batch_size {
                let bytes = match stream.next().await {
                    None => {
                        warn!("Journalctl process stopped.");
                        return true;
                    }
                    Some(Ok(text)) => text,
                    Some(Err(error)) => {
                        error!(
                            message = "Could not read from journald source.",
                            %error,
                        );
                        break;
                    }
                };

                let mut record = match decode_record(&bytes, self.remap_priority) {
                    Ok(record) => record,
                    Err(error) => {
                        emit!(JournaldInvalidRecord {
                            error,
                            text: String::from_utf8_lossy(&bytes).into_owned()
                        });
                        continue;
                    }
                };
                if let Some(tmp) = record.remove(&*CURSOR) {
                    *cursor = Some(tmp);
                }

                saw_record = true;

                let unit = record.get(&*SYSTEMD_UNIT);
                if filter_unit(unit, &self.include_units, &self.exclude_units) {
                    continue;
                }

                emit!(JournaldEventReceived {
                    byte_size: bytes.len()
                });

                match self.out.send(create_event(record)).await {
                    Ok(_) => {}
                    Err(error) => {
                        error!(message = "Could not send journald log.", %error);
                        // `out` channel is closed, don't restart journalctl.
                        return false;
                    }
                }
            }

            if saw_record {
                Self::save_checkpoint(checkpointer, &*cursor).await;
            }
        }
    }

    async fn save_checkpoint(checkpointer: &mut Checkpointer, cursor: &Option<String>) {
        if let Some(cursor) = cursor {
            if let Err(error) = checkpointer.set(cursor).await {
                error!(
                    message = "Could not set journald checkpoint.",
                    %error,
                    filename = ?checkpointer.filename,
                );
            }
        }
    }
}

/// A function that starts journalctl process.
/// Return a stream of output splitted by '\n', and a `StopJournalctlFn`.
///
/// Code uses `start_journalctl` below,
/// but we need this type to implement fake journald source in testing.
type StartJournalctlFn = Box<
    dyn Fn(
            &Option<String>, // cursor
        ) -> crate::Result<(BoxStream<'static, io::Result<Bytes>>, StopJournalctlFn)>
        + Send
        + Sync,
>;

type StopJournalctlFn = Box<dyn FnOnce() + Send>;

fn start_journalctl(
    path: &PathBuf,
    current_boot_only: bool,
    cursor: &Option<String>,
) -> crate::Result<(BoxStream<'static, io::Result<Bytes>>, StopJournalctlFn)> {
    let mut command = Command::new(path);
    command.stdout(Stdio::piped());
    command.arg("--follow");
    command.arg("--all");
    command.arg("--show-cursor");
    command.arg("--output=json");

    if current_boot_only {
        command.arg("--boot");
    }

    if let Some(cursor) = cursor {
        command.arg(format!("--after-cursor={}", cursor));
    } else {
        // journalctl --follow only outputs a few lines without a starting point
        command.arg("--since=2000-01-01");
    }

    let mut child = command.spawn().context(JournalctlSpawn)?;

    let stream = FramedRead::new(
        child.stdout.take().unwrap(),
        BytesDelimitedCodec::new(b'\n'),
    )
    .boxed();

    let pid = Pid::from_raw(child.id() as i32);
    let stop = Box::new(move || {
        let _ = kill(pid, Signal::SIGTERM);
    });

    Ok((stream, stop))
}

fn create_event(record: Record) -> Event {
    let mut log = LogEvent::from_iter(record);
    // Convert some journald-specific field names into Vector standard ones.
    if let Some(message) = log.remove(MESSAGE) {
        log.insert(log_schema().message_key(), message);
    }
    if let Some(host) = log.remove(HOSTNAME) {
        log.insert(log_schema().host_key(), host);
    }
    // Translate the timestamp, and so leave both old and new names.
    if let Some(Value::Bytes(timestamp)) = log
        .get(&*SOURCE_TIMESTAMP)
        .or_else(|| log.get(RECEIVED_TIMESTAMP))
    {
        if let Ok(timestamp) = String::from_utf8_lossy(&timestamp).parse::<u64>() {
            let timestamp = chrono::Utc.timestamp(
                (timestamp / 1_000_000) as i64,
                (timestamp % 1_000_000) as u32 * 1_000,
            );
            log.insert(log_schema().timestamp_key(), Value::Timestamp(timestamp));
        }
    }
    // Add source type
    log.try_insert(log_schema().source_type_key(), Bytes::from("journald"));

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
    use futures::Stream;
    use std::pin::Pin;
    use std::{
        io::{BufRead, BufReader, Cursor},
        task::{Context, Poll},
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
{"_SYSTEMD_UNIT":"syslog.service","MESSAGE":"Non-ASCII in other field","__CURSOR":"5","_SOURCE_REALTIME_TIMESTAMP":"1578529839140005","__REALTIME_TIMESTAMP":"1578529839140004","PRIORITY":"3","SYSLOG_RAW":[194,191,87,111,114,108,100,63]}
{"_SYSTEMD_UNIT":"NetworkManager.service","MESSAGE":"<info>  [1608278027.6016] dhcp-init: Using DHCP client 'dhclient'","__CURSOR":"6","_SOURCE_REALTIME_TIMESTAMP":"1578529839140005","__REALTIME_TIMESTAMP":"1578529839140004","PRIORITY":"6","SYSLOG_FACILITY":["DHCP4","DHCP6"]}
"#;

    struct FakeJournal {
        reader: BufReader<Cursor<&'static str>>,
    }

    impl FakeJournal {
        fn next(&mut self) -> Option<io::Result<Bytes>> {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => None,
                Ok(_) => {
                    line.pop();
                    Some(Ok(Bytes::from(line)))
                }
                Err(err) => Some(Err(err)),
            }
        }
    }

    impl Stream for FakeJournal {
        type Item = io::Result<Bytes>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<Self::Item>> {
            Poll::Ready(Pin::into_inner(self).next())
        }
    }

    impl FakeJournal {
        fn new(
            checkpoint: &Option<String>,
        ) -> (BoxStream<'static, io::Result<Bytes>>, StopJournalctlFn) {
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

            (Box::pin(journal), Box::new(|| ()))
        }
    }

    async fn run_journal(iunits: &[&str], xunits: &[&str], cursor: Option<&str>) -> Vec<Event> {
        let (tx, rx) = Pipeline::new_test();
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();

        let tempdir = tempdir().unwrap();
        let mut checkpoint_path = tempdir.path().to_path_buf();
        checkpoint_path.push(CHECKPOINT_FILENAME);

        let mut checkpointer = Checkpointer::new(checkpoint_path.clone())
            .await
            .expect("Creating checkpointer failed!");

        if let Some(cursor) = cursor {
            checkpointer
                .set(cursor)
                .await
                .expect("Could not set checkpoint");
        }

        let include_units: HashSet<String> = iunits.iter().map(|&s| s.into()).collect();
        let exclude_units: HashSet<String> = xunits.iter().map(|&s| s.into()).collect();

        let source = JournaldSource {
            include_units,
            exclude_units,
            checkpoint_path,
            batch_size: DEFAULT_BATCH_SIZE,
            remap_priority: true,
            out: tx,
        }
        .run_shutdown(
            shutdown,
            Box::new(|checkpoint| Ok(FakeJournal::new(checkpoint))),
        );
        tokio::spawn(source);

        delay_for(Duration::from_millis(100)).await;
        drop(trigger);

        timeout(Duration::from_secs(1), rx.collect()).await.unwrap()
    }

    #[tokio::test]
    async fn reads_journal() {
        let received = run_journal(&[], &[], None).await;
        assert_eq!(received.len(), 7);
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
        let received = run_journal(&["unit.service"], &[], None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(message(&received[0]), Value::Bytes("unit message".into()));
        assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140002000));
    }

    #[tokio::test]
    async fn excludes_units() {
        let received = run_journal(&[], &["unit.service", "badunit.service"], None).await;
        assert_eq!(received.len(), 5);
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
        assert_eq!(received.len(), 6);
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
    async fn parses_array_fields() {
        let received = run_journal(&["syslog.service"], &[], None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log()["SYSLOG_RAW"],
            Value::Bytes("¿World?".into())
        );
    }

    #[tokio::test]
    async fn parses_string_sequences() {
        let received = run_journal(&["NetworkManager.service"], &[], None).await;
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log()["SYSLOG_FACILITY"],
            Value::Bytes(r#"["DHCP4","DHCP6"]"#.into())
        );
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
        event.as_log()[log_schema().message_key()].clone()
    }

    fn timestamp(event: &Event) -> Value {
        event.as_log()[log_schema().timestamp_key()].clone()
    }

    fn value_ts(secs: i64, usecs: u32) -> Value {
        Value::Timestamp(chrono::Utc.timestamp(secs, usecs))
    }

    fn priority(event: &Event) -> Value {
        event.as_log()["PRIORITY"].clone()
    }
}
