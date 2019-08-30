use crate::{
    event,
    event::{Event, LogEvent, ValueKind},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use chrono::TimeZone;
use futures::{future, sync::mpsc, Future, Sink};
use journald::{Journal, Record};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Error;
use std::iter::FromIterator;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time;
use string_cache::DefaultAtom as Atom;
use tracing::{dispatcher, field};

lazy_static! {
    static ref MESSAGE: Atom = Atom::from("MESSAGE");
    static ref TIMESTAMP: Atom = Atom::from("_SOURCE_REALTIME_TIMESTAMP");
    static ref HOSTNAME: Atom = Atom::from("_HOSTNAME");
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub struct JournaldConfig {
    pub current_runtime_only: Option<bool>,
    pub local_only: Option<bool>,
    pub units: Vec<String>,
}

#[typetag::serde(name = "journald")]
impl SourceConfig for JournaldConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> Result<super::Source, String> {
        let local_only = self.local_only.unwrap_or(true);
        let runtime_only = self.current_runtime_only.unwrap_or(true);
        let journal = Journal::open(local_only, runtime_only).map_err(|err| format!("{}", err))?;

        // Map the given unit names into valid systemd units by
        // appending ".service" if no extension is present.
        let units = self
            .units
            .iter()
            .map(|unit| {
                if let Some(_) = unit.find('.') {
                    unit.into()
                } else {
                    format!("{}.service", unit)
                }
            })
            .collect::<HashSet<String>>();

        Ok(journald_source(journal, out, units))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

fn journald_source<J>(journal: J, out: mpsc::Sender<Event>, units: HashSet<String>) -> super::Source
where
    J: 'static + Iterator<Item = Result<Record, Error>> + Send,
{
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

    let out = out
        .sink_map_err(|_| ())
        .with(|record: Record| future::ok(create_event(record)));

    Box::new(future::lazy(move || {
        info!(message = "Starting journald server.",);

        let journald_server = JournaldServer {
            journal,
            units,
            channel: out,
            shutdown: shutdown_rx,
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
    }))
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
            match String::from_utf8_lossy(timestamp).parse::<u64>() {
                Ok(timestamp) => {
                    let timestamp = chrono::Utc.timestamp(
                        (timestamp / 1_000_000) as i64,
                        (timestamp % 1_000_000) as u32 * 1_000,
                    );
                    log.insert_explicit(event::TIMESTAMP.clone(), ValueKind::Timestamp(timestamp));
                }
                Err(_) => {}
            }
        }
    }
    log.into()
}

struct JournaldServer<J, T> {
    journal: J,
    units: HashSet<String>,
    channel: T,
    shutdown: std::sync::mpsc::Receiver<()>,
}

impl<J, T> JournaldServer<J, T>
where
    J: Iterator<Item = Result<Record, Error>>,
    T: Sink<SinkItem = Record, SinkError = ()>,
{
    pub fn run(mut self) {
        let timeout = time::Duration::from_millis(500); // arbitrary timeout
        let channel = &mut self.channel;
        loop {
            loop {
                let record = match self.journal.next() {
                    None => break,
                    Some(Ok(record)) => record,
                    Some(Err(err)) => {
                        error!(
                            message = "Could not read from journald source",
                            error = field::display(&err),
                        );
                        break;
                    }
                };
                if self.units.len() > 0 {
                    // Make sure the systemd unit is exactly one of the specified units
                    if let Some(unit) = record.get("_SYSTEMD_UNIT") {
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
            // FIXME: Checkpoint here
            match self.shutdown.recv_timeout(timeout) {
                Ok(()) => unreachable!(), // The sender should never actually send
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{block_on, runtime, shutdown_on_idle};
    use futures::stream::Stream;
    use std::io::Error;
    use std::iter::FromIterator;
    use std::time::{Duration, SystemTime};
    use stream_cancel::Tripwire;
    use tokio::util::FutureExt;

    #[derive(Default)]
    struct FakeJournal {
        records: Vec<Record>,
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
        type Item = Result<Record, Error>;
        fn next(&mut self) -> Option<Self::Item> {
            self.records.pop().map(|item| Ok(item))
        }
    }

    fn fake_journal() -> FakeJournal {
        let mut journal = FakeJournal::default();
        journal.push("unit.service", "unit message");
        journal.push("sysinit.target", "System Initialization");
        journal
    }

    fn run_journal(units: &[&str]) -> Vec<Event> {
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();

        let journal = fake_journal();
        let source = journald_source(
            journal,
            tx,
            HashSet::from_iter(units.into_iter().map(|&s| s.into())),
        );
        let mut rt = runtime();
        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        std::thread::sleep(Duration::from_millis(100));
        drop(trigger);
        shutdown_on_idle(rt);

        block_on(rx.collect().timeout(Duration::from_secs(1))).expect("Unclosed channel")
    }

    #[test]
    fn journald_source_works() {
        let received = run_journal(&[]);
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
        let received = run_journal(&["unit.service"]);
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log()[&event::MESSAGE],
            ValueKind::Bytes("unit message".into())
        );
    }
}
