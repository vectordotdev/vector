use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::Event,
    shutdown::ShutdownSignal,
    Pipeline,
};
use futures::{
    compat::Future01CompatExt,
    future::{FutureExt, TryFutureExt},
    stream::StreamExt,
};
use futures01::{future::Future, stream::iter_ok, Sink};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::interval;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorConfig {
    #[serde(default)]
    sequence: bool,
    lines: Vec<String>,
    #[serde(default)]
    batch_interval: Option<f64>,
    #[serde(default = "usize::max_value")]
    count: usize,
}

impl GeneratorConfig {
    #[allow(dead_code)] // to make check-component-features pass
    pub fn repeat(lines: Vec<String>, count: usize, batch_interval: Option<f64>) -> Self {
        Self {
            lines,
            count,
            batch_interval,
            ..Self::default()
        }
    }
}

inventory::submit! {
    SourceDescription::new::<GeneratorConfig>("generator")
}

#[typetag::serde(name = "generator")]
impl SourceConfig for GeneratorConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        Ok(self.clone().generator(shutdown, out))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "generator"
    }
}

impl GeneratorConfig {
    pub(self) fn generator(self, shutdown: ShutdownSignal, out: Pipeline) -> super::Source {
        Box::new(self.inner(shutdown, out).boxed().compat())
    }

    async fn inner(self, mut shutdown: ShutdownSignal, mut out: Pipeline) -> Result<(), ()> {
        let mut batch_interval = self
            .batch_interval
            .map(|i| interval(Duration::from_secs_f64(i)));
        let mut number: usize = 0;

        for _ in 0..self.count {
            if shutdown.poll().expect("polling shutdown").is_ready() {
                break;
            }

            if let Some(batch_interval) = &mut batch_interval {
                batch_interval.next().await;
            }

            let events = self
                .lines
                .iter()
                .map(|line| {
                    if self.sequence {
                        number += 1;
                        Event::from(&format!("{} {}", number, line)[..])
                    } else {
                        Event::from(&line[..])
                    }
                })
                .collect::<Vec<Event>>();
            let (sink, _) = out
                .send_all(iter_ok(events))
                .compat()
                .await
                .map_err(|error| error!(message="error sending generated lines", %error))?;
            out = sink;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event, shutdown::ShutdownSignal, test_util::runtime, Pipeline};
    use futures01::{stream::Stream, sync::mpsc, Async::*};
    use std::time::{Duration, Instant};

    fn runit(config: &str) -> mpsc::Receiver<Event> {
        let (tx, rx) = Pipeline::new_test();
        let mut rt = runtime();
        let config: GeneratorConfig = toml::from_str(config).unwrap();
        let source = config.generator(ShutdownSignal::noop(), tx);
        rt.block_on(source).unwrap();
        rx
    }

    #[test]
    fn copies_lines() {
        let message_key = event::log_schema().message_key();
        let mut rx = runit(
            r#"lines = ["one", "two"]
               count = 1"#,
        );

        for line in &["one", "two"] {
            let event = rx.poll().unwrap();
            match event {
                Ready(Some(event)) => {
                    let log = event.as_log();
                    let message = log[&message_key].to_string_lossy();
                    assert_eq!(message, *line);
                }
                Ready(None) => panic!("Premature end of input"),
                NotReady => panic!("Generator was not ready"),
            }
        }

        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[test]
    fn limits_count() {
        let mut rx = runit(
            r#"lines = ["one", "two"]
               count = 5"#,
        );

        for _ in 0..10 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[test]
    fn adds_sequence() {
        let message_key = event::log_schema().message_key();
        let mut rx = runit(
            r#"lines = ["one", "two"]
               count = 2
               sequence = true"#,
        );

        for line in &["1 one", "2 two", "3 one", "4 two"] {
            let event = rx.poll().unwrap();
            match event {
                Ready(Some(event)) => {
                    let log = event.as_log();
                    let message = log[&message_key].to_string_lossy();
                    assert_eq!(message, *line);
                }
                Ready(None) => panic!("Premature end of input"),
                NotReady => panic!("Generator was not ready"),
            }
        }

        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[test]
    fn obeys_batch_interval() {
        let start = Instant::now();
        let mut rx = runit(
            r#"lines = ["one", "two"]
               count = 3
               batch_interval = 1.0"#,
        );

        for _ in 0..6 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        assert_eq!(rx.poll().unwrap(), Ready(None));
        let duration = start.elapsed();
        assert!(duration >= Duration::from_secs(2));
    }
}
