use crate::{
    event::{self, Event},
    shutdown::ShutdownSignal,
    stream::StreamExt,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes::Bytes;
use futures::compat::Compat;
use futures01::{sync::mpsc, Future, Sink, Stream};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{io, sync::Mutex, thread};
use tokio::sync::broadcast::{channel, Sender};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("CRITICAL_SECTION poisoned"))]
    CriticalSectionPoisoned,
}

lazy_static! {
    static ref CRITICAL_SECTION: Mutex<Option<Sender<Bytes>>> = Mutex::default();
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct StdinConfig {
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    pub host_key: Option<String>,
}

impl Default for StdinConfig {
    fn default() -> Self {
        StdinConfig {
            max_length: default_max_length(),
            host_key: None,
        }
    }
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

inventory::submit! {
    SourceDescription::new::<StdinConfig>("stdin")
}

#[typetag::serde(name = "stdin")]
impl SourceConfig for StdinConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        stdin_source(io::BufReader::new(io::stdin()), self.clone(), shutdown, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "stdin"
    }
}

pub fn stdin_source<R>(
    stdin: R,
    config: StdinConfig,
    shutdown: ShutdownSignal,
    out: mpsc::Sender<Event>,
) -> crate::Result<super::Source>
where
    R: Send + io::BufRead + 'static,
{
    // The idea is to have one dedicated future for reading stdin running in the background,
    // and the sources would recieve the lines thorugh a multi consumer channel.
    //
    // Implemented solution relies on having a copy of optional sender behind a global mutex,
    // and have stdin sources and background thread synchronize on it.
    //
    // When source is built it must:
    // 1. enter critical section by locking mutex.
    // 2. if sender isn't present start background thread and set sender.
    // 3. gain receiver by calling subscribe on the sender.
    // 4. release lock.
    //
    // When source has finished it should just drop it's receiver.
    //
    // Background thread should be started with copy of the sender.
    //
    // Once background thread wants to stop it must:
    // 1. enter critical section by locking mutex.
    // 2. if there are receivers it must abort the stop.
    // 3. remove sender.
    // 4. release lock.
    //
    // Although it's possible to implement this in a lock free, maybe even wait free manner,
    // this should be easier to reason about and performance shouldn't suffer since this procedure
    // is cold compared to the rest of the source.

    let host_key = config
        .host_key
        .clone()
        .unwrap_or(event::log_schema().host_key().to_string());
    let hostname = hostname::get_hostname();

    let mut guard = CRITICAL_SECTION
        .lock()
        .map_err(|_| BuildError::CriticalSectionPoisoned)?;
    let receiver = match guard.as_ref() {
        Some(sender) => sender.subscribe(),
        None => {
            let (sender, receiver) = channel(1024);
            *guard = Some(sender.clone());

            // Start the background thread
            thread::spawn(move || {
                info!("Capturing STDIN.");

                for line in stdin.lines() {
                    match line {
                        Err(e) => {
                            error!(message = "Unable to read from source.", error = %e);
                            break;
                        }
                        Ok(line) => {
                            if sender.send(Bytes::from(line)).is_err() {
                                // There are no active receivers.
                                // Try to stop.
                                let mut guard =
                                    CRITICAL_SECTION.lock().expect("CRITICAL_SECTION poisoned");

                                if sender.receiver_count() == 0 {
                                    guard.take();
                                    return;
                                }

                                // A new receiver has shown up.

                                // It's fine not to resend the line since it came from
                                // before this new receiver has shown up.
                            }
                        }
                    }
                }

                CRITICAL_SECTION
                    .lock()
                    .expect("CRITICAL_SECTION poisoned")
                    .take();
            });

            receiver
        }
    };
    std::mem::drop(guard);

    Ok(Box::new(
        Compat::new(receiver)
            .take_until(shutdown)
            .map(move |line| create_event(line, &host_key, &hostname))
            .map_err(|e| error!("error reading line: {:?}", e))
            .forward(
                out.sink_map_err(|e| error!(message = "Unable to send event to out.", error = %e)),
            )
            .map(|_| info!("finished sending")),
    ))
}

fn create_event(line: Bytes, host_key: &str, hostname: &Option<String>) -> Event {
    let mut event = Event::from(line);

    // Add source type
    event
        .as_mut_log()
        .insert(event::log_schema().source_type_key(), "stdin");

    if let Some(hostname) = &hostname {
        event.as_mut_log().insert(host_key, hostname.clone());
    }

    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event, test_util::runtime};
    use futures01::sync::mpsc;
    use futures01::Async::*;
    use std::io::Cursor;

    #[test]
    fn stdin_create_event() {
        let line = Bytes::from("hello world");
        let host_key = "host".to_string();
        let hostname = Some("Some.Machine".to_string());

        let event = create_event(line, &host_key, &hostname);
        let log = event.into_log();

        assert_eq!(log[&"host".into()], "Some.Machine".into());
        assert_eq!(
            log[&event::log_schema().message_key()],
            "hello world".into()
        );
        assert_eq!(log[event::log_schema().source_type_key()], "stdin".into());
    }

    #[test]
    fn stdin_decodes_line() {
        crate::test_util::trace_init();
        let (tx, mut rx) = mpsc::channel(10);
        let config = StdinConfig::default();
        let buf = Cursor::new(String::from("hello world\nhello world again"));

        let mut rt = runtime();
        let source = stdin_source(buf, config, ShutdownSignal::noop(), tx).unwrap();

        rt.block_on(source).unwrap();

        let event = rx.poll().unwrap();

        assert!(event.is_ready());
        assert_eq!(
            Ready(Some("hello world".into())),
            event.map(|event| event
                .map(|event| event.as_log()[&event::log_schema().message_key()].to_string_lossy()))
        );

        let event = rx.poll().unwrap();
        assert!(event.is_ready());
        assert_eq!(
            Ready(Some("hello world again".into())),
            event.map(|event| event
                .map(|event| event.as_log()[&event::log_schema().message_key()].to_string_lossy()))
        );

        let event = rx.poll().unwrap();
        assert!(event.is_ready());
        assert_eq!(Ready(None), event);
    }
}
