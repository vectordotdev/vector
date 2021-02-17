use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::Event,
    internal_events::GeneratorEventProcessed,
    shutdown::ShutdownSignal,
    Pipeline,
};
use fakedata::logs::*;
use futures::{stream::StreamExt, SinkExt};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::task::Poll;
use tokio::time::{interval, Duration};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GeneratorConfig {
    #[serde(alias = "batch_interval")]
    interval: Option<f64>,
    #[serde(default = "usize::max_value")]
    count: usize,
    #[serde(flatten)]
    format: OutputFormat,
}

#[derive(Debug, PartialEq, Snafu)]
pub enum GeneratorConfigError {
    #[snafu(display("A non-empty list of lines is required for the shuffle format"))]
    ShuffleGeneratorItemsEmpty,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum OutputFormat {
    #[derivative(Default)]
    Shuffle {
        #[serde(default)]
        sequence: bool,
        lines: Vec<String>,
    },
    ApacheCommon,
    ApacheError,
    #[serde(alias = "rfc5424")]
    Syslog,
    #[serde(alias = "rfc3164")]
    BsdSyslog,
    Json,
}

impl OutputFormat {
    fn generate_event(&self, n: usize) -> Event {
        emit!(GeneratorEventProcessed);

        let line = match self {
            Self::Shuffle {
                sequence,
                ref lines,
            } => Self::shuffle_generate(*sequence, lines, n),
            Self::ApacheCommon => apache_common_log_line(),
            Self::ApacheError => apache_error_log_line(),
            Self::Syslog => syslog_5424_log_line(),
            Self::BsdSyslog => syslog_3164_log_line(),
            Self::Json => json_log_line(),
        };
        Event::from(line)
    }

    fn shuffle_generate(sequence: bool, lines: &[String], n: usize) -> String {
        // unwrap can be called here because `lines` can't be empty
        let line = lines.choose(&mut rand::thread_rng()).unwrap();

        if sequence {
            format!("{} {}", n, line)
        } else {
            line.into()
        }
    }

    // Ensures that the `lines` list is non-empty if `Shuffle` is chosen
    pub(self) fn validate(&self) -> Result<(), GeneratorConfigError> {
        match self {
            Self::Shuffle { lines, .. } => {
                if lines.is_empty() {
                    Err(GeneratorConfigError::ShuffleGeneratorItemsEmpty)
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
            format: OutputFormat::Shuffle {
                lines,
                sequence: false,
            },
        }
    }

    async fn inner(self, mut shutdown: ShutdownSignal, mut out: Pipeline) -> Result<(), ()> {
        let mut interval = self.interval.map(|i| interval(Duration::from_secs_f64(i)));

        for n in 0..self.count {
            if matches!(futures::poll!(&mut shutdown), Poll::Ready(_)) {
                break;
            }

            if let Some(interval) = &mut interval {
                interval.next().await;
            }

            let event = self.format.generate_event(n);

            out.send(event)
                .await
                .map_err(|_: crate::pipeline::ClosedError| {
                    error!(message = "Failed to forward events; downstream is closed.");
                })?;
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
        self.format.validate()?;
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
    use std::time::{Duration, Instant};
    use tokio::sync::mpsc;

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
    fn config_shuffle_lines_not_empty() {
        let empty_lines: Vec<String> = Vec::new();

        let errant_config = GeneratorConfig {
            format: OutputFormat::Shuffle {
                sequence: false,
                lines: empty_lines,
            },
            ..GeneratorConfig::default()
        };

        assert_eq!(
            errant_config.format.validate(),
            Err(GeneratorConfigError::ShuffleGeneratorItemsEmpty)
        );
    }

    #[tokio::test]
    async fn shuffle_generator_copies_lines() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"format = "shuffle"
               lines = ["one", "two", "three", "four"]
               count = 5"#,
        )
        .await;

        let lines = &["one", "two", "three", "four"];

        for _ in 0..5 {
            let event = rx.try_recv().unwrap();
            let log = event.as_log();
            let message = log[&message_key].to_string_lossy();
            assert!(lines.contains(&&*message));
        }

        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn shuffle_generator_limits_count() {
        let mut rx = runit(
            r#"format = "shuffle"
               lines = ["one", "two"]
               count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(matches!(rx.try_recv(), Ok(_)));
        }
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn shuffle_generator_adds_sequence() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"format = "shuffle"
               lines = ["one", "two"]
               sequence = true
               count = 5"#,
        )
        .await;

        for n in 0..5 {
            let event = rx.try_recv().unwrap();
            let log = event.as_log();
            let message = log[&message_key].to_string_lossy();
            assert!(message.starts_with(&n.to_string()));
        }

        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn shuffle_generator_obeys_interval() {
        let start = Instant::now();
        let mut rx = runit(
            r#"format = "shuffle"
               lines = ["one", "two"]
               count = 3
               interval = 1.0"#,
        )
        .await;

        for _ in 0..3 {
            assert!(matches!(rx.try_recv(), Ok(_)));
        }
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));

        let duration = start.elapsed();
        assert!(duration >= Duration::from_secs(2));
    }

    #[tokio::test]
    async fn apache_common_format_generates_output() {
        let mut rx = runit(
            r#"format = "apache_common"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(matches!(rx.try_recv(), Ok(_)));
        }
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn apache_error_format_generates_output() {
        let mut rx = runit(
            r#"format = "apache_error"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(matches!(rx.try_recv(), Ok(_)));
        }
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn syslog_5424_format_generates_output() {
        let mut rx = runit(
            r#"format = "syslog"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(matches!(rx.try_recv(), Ok(_)));
        }
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn syslog_3164_format_generates_output() {
        let mut rx = runit(
            r#"format = "bsd_syslog"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(matches!(rx.try_recv(), Ok(_)));
        }
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn json_format_generates_output() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"format = "json"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            let event = rx.try_recv().unwrap();
            let log = event.as_log();
            let message = log[&message_key].to_string_lossy();
            assert!(serde_json::from_str::<serde_json::Value>(&message).is_ok());
        }
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }
}
