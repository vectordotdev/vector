use crate::{
    event,
    event::{Event, LogEvent, ValueKind},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use chrono::TimeZone;
use futures::{future, sync::mpsc, Future, Sink};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::io::Error;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time;
use string_cache::DefaultAtom as Atom;
use systemd::journal::{Journal, JournalFiles, JournalRecord};
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
}

#[typetag::serde(name = "journald")]
impl SourceConfig for JournaldConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> Result<super::Source, String> {
        let journal = Journal::open(
            JournalFiles::All,
            self.current_runtime_only.unwrap_or(true),
            self.local_only.unwrap_or(true),
        )
        .map_err(|err| format!("{}", err))?;

        Ok(journald_source(journal, out))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

fn journald_source<J: 'static + JournaldSource + Send>(
    journal: J,
    out: mpsc::Sender<Event>,
) -> super::Source {
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

    let out = out
        .sink_map_err(|_| ())
        .with(|record: JournalRecord| future::ok(create_event(record)));

    Box::new(future::lazy(move || {
        info!(message = "Starting journald server.",);

        let journald_server = JournaldServer {
            journal,
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

fn create_event(record: JournalRecord) -> Event {
    let mut log = LogEvent::from(record.into_iter());
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

trait JournaldSource {
    fn next_record(&mut self) -> Result<Option<JournalRecord>, Error>;
}

impl JournaldSource for Journal {
    fn next_record(&mut self) -> Result<Option<JournalRecord>, Error> {
        Journal::next_record(self)
    }
}

struct JournaldServer<J, T> {
    journal: J,
    channel: T,
    shutdown: std::sync::mpsc::Receiver<()>,
}

impl<J: JournaldSource, T: Sink<SinkItem = JournalRecord, SinkError = ()>> JournaldServer<J, T> {
    pub fn run(mut self) {
        let timeout = time::Duration::from_millis(500); // arbitrary timeout
        let channel = &mut self.channel;
        loop {
            loop {
                let record = match self.journal.next_record() {
                    Ok(Some(record)) => record,
                    Ok(None) => break,
                    Err(err) => {
                        error!(
                            message = "Could not read from journald source",
                            error = field::display(&err),
                        );
                        break;
                    }
                };
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
mod tests {}
