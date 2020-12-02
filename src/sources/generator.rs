use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::Event,
    internal_events::GeneratorEventProcessed,
    shutdown::ShutdownSignal,
    sources::util::fake::{apache_common_log_line, apache_error_log_line, syslog_log_line},
    Pipeline,
};
use futures::{compat::Future01CompatExt, stream::StreamExt};
use futures01::{stream::iter_ok, Sink};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::task::Poll;
use tokio::time::{interval, Duration};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GeneratorConfig {
    #[serde(default)]
    batch_interval: Option<f64>,
    #[serde(default = "usize::max_value")]
    count: usize,
    #[serde(flatten)]
    format: OutputFormat,
}

#[derive(Debug, Snafu)]
pub enum GeneratorConfigErrors {
    #[snafu(display("Expected a non-empty items list for round_robin but got an empty list"))]
    RoundRobinItemsEmpty,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum OutputFormat {
    #[derivative(Default)]
    RoundRobin {
        #[serde(default)]
        sequence: bool,
        #[serde(alias = "lines")]
        items: Vec<String>,
    },
    ApacheCommon,
    ApacheError,
    #[serde(alias = "rfc5424")]
    Syslog,
}

struct Generator;

impl Generator {
    async fn generate(
        config: GeneratorConfig,
        mut shutdown: ShutdownSignal,
        mut out: Pipeline,
    ) -> Result<(), ()> {
        let mut batch_interval = config
            .batch_interval
            .map(|i| interval(Duration::from_secs_f64(i)));

        for n in 0..config.count {
            if matches!(futures::poll!(&mut shutdown), Poll::Ready(_)) {
                break;
            }

            if let Some(batch_interval) = &mut batch_interval {
                batch_interval.next().await;
            }

            let events = config.format.generate_events(n);

            let (sink, _) = out
                .clone()
                .send_all(iter_ok(events))
                .compat()
                .await
                .map_err(|error| error!(message="Error sending generated lines.", %error))?;
            out = sink;
        }

        Ok(())
    }
}

impl OutputFormat {
    fn generate_events(&self, n: usize) -> Vec<Event> {
        emit!(GeneratorEventProcessed);

        let events_from_log_line = |log: String| -> Vec<Event> { vec![Event::from(log)] };

        match self {
            Self::RoundRobin {
                sequence,
                ref items,
            } => Self::round_robin_generate(sequence, items, n),
            Self::ApacheCommon => events_from_log_line(apache_common_log_line()),
            Self::ApacheError => events_from_log_line(apache_error_log_line()),
            Self::Syslog => events_from_log_line(syslog_log_line()),
        }
    }

    fn round_robin_generate(sequence: &bool, items: &[String], n: usize) -> Vec<Event> {
        let line: String = items.choose(&mut rand::thread_rng()).unwrap().into();

        let event = if *sequence {
            Event::from(&format!("{} {}", n, line)[..])
        } else {
            Event::from(&line[..])
        };

        vec![event]
    }

    // Ensures that the items list is non-empty if RoundRobin is chosen
    fn items_empty(&self) -> bool {
        match self {
            Self::RoundRobin { items, .. } => items.is_empty(),
            _ => false,
        }
    }
}

impl GeneratorConfig {
    pub(self) fn generator(self, shutdown: ShutdownSignal, out: Pipeline) -> super::Source {
        Box::pin(self.inner(shutdown, out))
    }

    #[allow(dead_code)] // to make check-component-features pass
    pub fn repeat(items: Vec<String>, count: usize, batch_interval: Option<f64>) -> Self {
        Self {
            count,
            batch_interval,
            format: OutputFormat::RoundRobin {
                items,
                sequence: false,
            },
        }
    }

    async fn inner(self, shutdown: ShutdownSignal, out: Pipeline) -> Result<(), ()> {
        Generator::generate(self, shutdown, out).await
    }
}

inventory::submit! {
    SourceDescription::new::<GeneratorConfig>("generator")
}

impl_generate_config_from_default!(GeneratorConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "generator")]
impl SourceConfig for GeneratorConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        if self.format.items_empty() {
            return Err(Box::new(GeneratorConfigErrors::RoundRobinItemsEmpty));
        }

        Ok(self.clone().generator(shutdown, out))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "generator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::log_schema, shutdown::ShutdownSignal, Pipeline};
    use futures01::{stream::Stream, sync::mpsc, Async::*};
    use std::time::{Duration, Instant};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<GeneratorConfig>();
    }

    async fn runit(config: &str) -> mpsc::Receiver<Event> {
        let (tx, rx) = Pipeline::new_test();
        let config: GeneratorConfig = toml::from_str(config).unwrap();
        config.generator(ShutdownSignal::noop(), tx).await.unwrap();
        rx
    }

    async fn done_generating(mut rx: mpsc::Receiver<Event>) {
        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[tokio::test]
    async fn round_robin_copies_lines() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"format = "round_robin"
               items = ["one", "two"]
               count = 1"#,
        )
        .await;

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

        done_generating(rx).await;
    }

    #[tokio::test]
    async fn round_robin_limits_count() {
        let mut rx = runit(
            r#"format = "round_robin"
               items = ["one", "two"]
               count = 5"#,
        )
        .await;

        for _ in 0..10 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        done_generating(rx).await;
    }

    #[tokio::test]
    async fn round_robin_adds_sequence() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"format = "round_robin"
               items = ["one", "two"]
               sequence = true
               count = 2"#,
        )
        .await;

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
        done_generating(rx).await;
    }

    #[tokio::test]
    async fn round_robin_obeys_batch_interval() {
        let start = Instant::now();
        let mut rx = runit(
            r#"format = "round_robin"
               items = ["one", "two"]
               count = 3
               batch_interval = 1.0"#,
        )
        .await;

        for _ in 0..6 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        done_generating(rx).await;

        let duration = start.elapsed();
        assert!(duration >= Duration::from_secs(2));
    }
}
