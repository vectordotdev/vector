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
use serde::{Deserialize, Serialize};
use std::task::Poll;
use tokio::time::{interval, Duration, Interval};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GeneratorConfig {
    #[serde(default)]
    batch_interval: Option<f64>,
    #[serde(default = "usize::max_value")]
    count: usize,
    #[serde(flatten)]
    format: OutputFormat,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum OutputFormat {
    #[derivative(Default)]
    RoundRobin {
        #[serde(default)]
        sequence: bool,
        items: Vec<String>,
    },
    ApacheCommon,
    ApacheError,
    Syslog,
}

struct Generator;

impl Generator {
    async fn generate(
        config: GeneratorConfig,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> Result<(), ()> {
        match config.clone().format {
            OutputFormat::RoundRobin {
                sequence,
                ref items,
            } => round_robin_generate(config, &sequence, &items, shutdown, out).await,
            OutputFormat::ApacheCommon => apache_common_generate(config, shutdown, out).await,
            OutputFormat::ApacheError => apache_error_generate(config, shutdown, out).await,
            OutputFormat::Syslog => syslog_generate(config, shutdown, out).await,
        }
    }
}

async fn apache_common_generate(
    config: GeneratorConfig,
    mut shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<(), ()> {
    let mut batch_interval = get_batch_interval(config.batch_interval);

    for _ in 0..config.count {
        if matches!(futures::poll!(&mut shutdown), Poll::Ready(_)) {
            break;
        }

        if let Some(batch_interval) = &mut batch_interval {
            batch_interval.next().await;
        }

        let events = events_from_log_line(apache_common_log_line());

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

async fn apache_error_generate(
    config: GeneratorConfig,
    mut shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<(), ()> {
    let mut batch_interval = get_batch_interval(config.batch_interval);

    for _ in 0..config.count {
        if matches!(futures::poll!(&mut shutdown), Poll::Ready(_)) {
            break;
        }

        if let Some(batch_interval) = &mut batch_interval {
            batch_interval.next().await;
        }

        let events = events_from_log_line(apache_error_log_line());

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

async fn round_robin_generate(
    config: GeneratorConfig,
    sequence: &bool,
    items: &Vec<String>,
    mut shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<(), ()> {
    let mut batch_interval = get_batch_interval(config.batch_interval);
    let mut number: usize = 0;

    for _ in 0..config.count {
        if matches!(futures::poll!(&mut shutdown), Poll::Ready(_)) {
            break;
        }

        if let Some(batch_interval) = &mut batch_interval {
            batch_interval.next().await;
        }

        let events = items
            .to_vec()
            .iter()
            .map(|line| {
                emit!(GeneratorEventProcessed);

                if *sequence {
                    number += 1;
                    Event::from(&format!("{} {}", number, line)[..])
                } else {
                    Event::from(&line[..])
                }
            })
            .collect::<Vec<Event>>();

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

async fn syslog_generate(
    config: GeneratorConfig,
    mut shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<(), ()> {
    let mut batch_interval = get_batch_interval(config.batch_interval);

    for _ in 0..config.count {
        if matches!(futures::poll!(&mut shutdown), Poll::Ready(_)) {
            break;
        }

        if let Some(batch_interval) = &mut batch_interval {
            batch_interval.next().await;
        }

        let events = events_from_log_line(syslog_log_line());

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

fn events_from_log_line(log: String) -> Vec<Event> {
    vec![Event::from(log)]
}

fn get_batch_interval(i: Option<f64>) -> Option<Interval> {
    i.map(|i| interval(Duration::from_secs_f64(i)))
}

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

    #[tokio::test]
    async fn copies_lines() {
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

        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[tokio::test]
    async fn limits_count() {
        let mut rx = runit(
            r#"format = "round_robin"
               items = ["one", "two"]
               count = 5"#,
        )
        .await;

        for _ in 0..10 {
            assert!(matches!(rx.poll().unwrap(), Ready(Some(_))));
        }
        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[tokio::test]
    async fn adds_sequence() {
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

        assert_eq!(rx.poll().unwrap(), Ready(None));
    }

    #[tokio::test]
    async fn obeys_batch_interval() {
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
        assert_eq!(rx.poll().unwrap(), Ready(None));
        let duration = start.elapsed();
        assert!(duration >= Duration::from_secs(2));
    }
}
