use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::Event,
    internal_events::GeneratorEventProcessed,
    shutdown::ShutdownSignal,
    sources::util::fake::{apache_common_log_line, apache_error_log_line, syslog_5424_log_line},
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
    #[serde(default, alias = "batch_interval")]
    interval: Option<f64>,
    #[serde(default = "usize::max_value")]
    count: usize,
    #[serde(flatten)]
    format: OutputFormat,
}

#[derive(Debug, PartialEq, Snafu)]
pub enum GeneratorConfigError {
    #[snafu(display("Expected a non-empty list of lines for round_robin but got an empty list"))]
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
        lines: Vec<String>,
    },
    ApacheCommon,
    ApacheError,
    #[serde(alias = "rfc5424")]
    Syslog,
}

impl OutputFormat {
    fn generate_events(&self, n: usize) -> Vec<Event> {
        emit!(GeneratorEventProcessed);

        let events_from_log_line = |log: String| -> Vec<Event> { vec![Event::from(log)] };

        match self {
            Self::RoundRobin {
                sequence,
                ref lines,
            } => Self::round_robin_generate(sequence, lines, n),
            Self::ApacheCommon => events_from_log_line(apache_common_log_line()),
            Self::ApacheError => events_from_log_line(apache_error_log_line()),
            Self::Syslog => events_from_log_line(syslog_5424_log_line()),
        }
    }

    fn round_robin_generate(sequence: &bool, lines: &[String], n: usize) -> Vec<Event> {
        // unwrap can be called here because lines cannot be empty
        let line: String = lines.choose(&mut rand::thread_rng()).unwrap().into();

        let event = if *sequence {
            Event::from(&format!("{} {}", n, line)[..])
        } else {
            Event::from(&line[..])
        };

        vec![event]
    }

    // Ensures that the lines list is non-empty if RoundRobin is chosen
    pub(self) fn validate(&self) -> Result<(), GeneratorConfigError> {
        match self {
            Self::RoundRobin { lines, .. } => {
                if lines.is_empty() {
                    Err(GeneratorConfigError::RoundRobinItemsEmpty)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
}

impl GeneratorConfig {
    pub(self) fn generator(self, shutdown: ShutdownSignal, out: Pipeline) -> super::Source {
        Box::pin(self.inner(shutdown, out))
    }

    #[allow(dead_code)] // to make check-component-features pass
    pub fn repeat(lines: Vec<String>, count: usize, interval: Option<f64>) -> Self {
        Self {
            count,
            interval,
            format: OutputFormat::RoundRobin {
                lines,
                sequence: false,
            },
        }
    }

    async fn inner(self, mut shutdown: ShutdownSignal, mut out: Pipeline) -> Result<(), ()> {
        let mut interval = self
            .interval
            .map(|i| interval(Duration::from_secs_f64(i)));

        for n in 0..self.count {
            if matches!(futures::poll!(&mut shutdown), Poll::Ready(_)) {
                break;
            }

            if let Some(interval) = &mut interval {
                interval.next().await;
            }

            let events = self.format.generate_events(n);

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
        if let Err(e) = self.format.validate() {
            return Err(Box::new(e));
        };

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

    #[test]
    fn config_round_robin_lines_not_empty() {
        let empty_lines: Vec<String> = Vec::new();

        let errant_config = GeneratorConfig {
            format: OutputFormat::RoundRobin {
                sequence: false,
                lines: empty_lines,
            },
            ..GeneratorConfig::default()
        };

        assert_eq!(
            errant_config.format.validate(),
            Err(GeneratorConfigError::RoundRobinItemsEmpty)
        );
    }

    #[tokio::test]
    async fn round_robin_copies_lines() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"format = "round_robin"
               lines = ["one", "two", "three", "four"]
               count = 5"#,
        )
        .await;

        let lines = &["one", "two", "three", "four"];

        for _ in 0..5 {
            let event = rx.poll().unwrap();
            match event {
                Ready(Some(event)) => {
                    let log = event.as_log();
                    let message = log[&message_key].to_string_lossy();
                    assert!(lines.contains(&&*message));
                }
                Ready(None) => panic!("Premature end of input"),
                NotReady => panic!("Generator was not ready"),
            }
        }

        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[tokio::test]
    async fn round_robin_limits_count() {
        let mut rx = runit(
            r#"format = "round_robin"
               lines = ["one", "two"]
               count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[tokio::test]
    async fn round_robin_adds_sequence() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"format = "round_robin"
               lines = ["one", "two"]
               sequence = true
               count = 5"#,
        )
        .await;

        for n in 0..5 {
            let event = rx.poll().unwrap();
            match event {
                Ready(Some(event)) => {
                    let log = event.as_log();
                    let message = log[&message_key].to_string_lossy();
                    assert!(message.starts_with(&n.to_string()));
                }
                Ready(None) => panic!("Premature end of input"),
                NotReady => panic!("Generator was not ready"),
            }
        }
        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[tokio::test]
    async fn round_robin_obeys_interval() {
        let start = Instant::now();
        let mut rx = runit(
            r#"format = "round_robin"
               lines = ["one", "two"]
               count = 3
               interval = 1.0"#,
        )
        .await;

        for _ in 0..3 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        assert_eq!(rx.poll().unwrap(), Ready(None));

        let duration = start.elapsed();
        assert!(duration >= Duration::from_secs(2));
    }

    #[tokio::test]
    async fn apache_common_generates_output() {
        let mut rx = runit(
            r#"format = "apache_common"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[tokio::test]
    async fn apache_error_generates_output() {
        let mut rx = runit(
            r#"format = "apache_error"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[tokio::test]
    async fn syslog_generates_output() {
        let mut rx = runit(
            r#"format = "syslog"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        assert_eq!(rx.poll().unwrap(), Ready(None));
    }
}
