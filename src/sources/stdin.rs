use crate::{
    event::{self, Event},
    shutdown::ShutdownSignal,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes::Bytes;
use futures01::{future, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::{io, thread, time::Duration};

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
        _shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        Ok(stdin_source(
            io::BufReader::new(io::stdin()),
            self.clone(),
            out,
        ))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "stdin"
    }
}

pub fn stdin_source<R>(stdin: R, config: StdinConfig, out: mpsc::Sender<Event>) -> super::Source
where
    R: Send + io::BufRead + 'static,
{
    Box::new(future::lazy(move || {
        info!("Capturing STDIN");

        let host_key = config
            .host_key
            .clone()
            .unwrap_or(event::log_schema().host_key().to_string());
        let hostname = hostname::get_hostname();
        let (mut tx, rx) = futures01::sync::mpsc::channel(1024);

        thread::spawn(move || {
            for line in stdin.lines() {
                match line {
                    Err(e) => {
                        error!(message = "Unable to read from source.", error = %e);
                        break;
                    }
                    Ok(string_data) => {
                        let msg = Bytes::from(string_data);
                        while let Err(e) = tx.try_send(msg.clone()) {
                            if e.is_full() {
                                thread::sleep(Duration::from_millis(10));
                                continue;
                            }
                            error!(message = "Unable to send event.", error = %e);
                            break;
                        }
                    }
                }
            }
        });

        rx.map(move |line| create_event(line, &host_key, &hostname))
            .map_err(|e| error!("error reading line: {:?}", e))
            .forward(
                out.sink_map_err(|e| error!(message = "Unable to send event to out.", error = %e)),
            )
            .map(|_| info!("finished sending"))
    }))
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
    use crate::event;
    use futures01::sync::mpsc;
    use futures01::Async::*;
    use std::io::Cursor;
    use tokio01::runtime::current_thread::Runtime;

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
        let (tx, mut rx) = mpsc::channel(10);
        let config = StdinConfig::default();
        let buf = Cursor::new(String::from("hello world\nhello world again"));

        let mut rt = Runtime::new().unwrap();
        let source = stdin_source(buf, config, tx);

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
