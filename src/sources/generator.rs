use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::Event,
    internal_events::GeneratorEventProcessed,
    shutdown::ShutdownSignal,
    Pipeline,
};
use futures::{stream::StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use std::task::Poll;
use tokio::time::{interval, Duration};

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
        Box::pin(self.inner(shutdown, out))
    }

    async fn inner(self, mut shutdown: ShutdownSignal, mut out: Pipeline) -> Result<(), ()> {
        let mut batch_interval = self
            .batch_interval
            .map(|i| interval(Duration::from_secs_f64(i)));
        let mut number: usize = 0;

        for _ in 0..self.count {
            if matches!(futures::poll!(&mut shutdown), Poll::Ready(_)) {
                break;
            }

            if let Some(batch_interval) = &mut batch_interval {
                batch_interval.next().await;
            }

            let events = self
                .lines
                .iter()
                .map(|line| {
                    emit!(GeneratorEventProcessed);

                    if self.sequence {
                        number += 1;
                        Event::from(&format!("{} {}", number, line)[..])
                    } else {
                        Event::from(&line[..])
                    }
                })
                .map(Ok)
                .collect::<Vec<Result<Event, _>>>();

            out.send_all(&mut futures::stream::iter(events))
                .await
                .map_err(|_: crate::pipeline::ClosedError| {
                    error!(message = "Failed to forward events; downstream is closed.");
                })?;
        }
        Ok(())
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

    #[tokio::test]
    async fn copies_lines() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"lines = ["one", "two"]
               count = 1"#,
        )
        .await;

        for line in &["one", "two"] {
            let event = rx.try_recv().unwrap();
            let log = event.as_log();
            let message = log[&message_key].to_string_lossy();
            assert_eq!(message, *line);
        }

        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn limits_count() {
        let mut rx = runit(
            r#"lines = ["one", "two"]
               count = 5"#,
        )
        .await;

        for _ in 0..10 {
            assert!(matches!(rx.try_recv(), Ok(_)));
        }
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn adds_sequence() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"lines = ["one", "two"]
               count = 2
               sequence = true"#,
        )
        .await;

        for line in &["1 one", "2 two", "3 one", "4 two"] {
            let event = rx.try_recv().unwrap();
            let log = event.as_log();
            let message = log[&message_key].to_string_lossy();
            assert_eq!(message, *line);
        }

        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
    }

    #[tokio::test]
    async fn obeys_batch_interval() {
        let start = Instant::now();
        let mut rx = runit(
            r#"lines = ["one", "two"]
               count = 3
               batch_interval = 1.0"#,
        )
        .await;

        for _ in 0..6 {
            assert!(matches!(rx.try_recv(), Ok(_)));
        }
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Closed));
        let duration = start.elapsed();
        assert!(duration >= Duration::from_secs(2));
    }
}
