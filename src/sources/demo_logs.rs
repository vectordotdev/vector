use std::task::Poll;

use bytes::Bytes;
use chrono::Utc;
use fakedata::logs::*;
use futures::{stream, StreamExt};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tokio::time::{self, Duration};
use tokio_util::codec::FramedRead;

use crate::{
    codecs::{
        self,
        decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    },
    config::{log_schema, DataType, Output, SourceConfig, SourceContext, SourceDescription},
    internal_events::{BytesReceived, DemoLogsEventProcessed, EventsReceived, StreamClosedError},
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::util::StreamDecodingError,
    SourceSender,
};

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(default)]
pub struct DemoLogsConfig {
    #[serde(alias = "batch_interval")]
    #[derivative(Default(value = "default_interval()"))]
    pub interval: f64,
    #[derivative(Default(value = "default_count()"))]
    pub count: usize,
    #[serde(flatten)]
    pub format: OutputFormat,
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: Box<dyn FramingConfig>,
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: Box<dyn DeserializerConfig>,
}

const fn default_interval() -> f64 {
    1.0
}

const fn default_count() -> usize {
    isize::MAX as usize
}

#[derive(Debug, PartialEq, Snafu)]
pub enum DemoLogsConfigError {
    #[snafu(display("A non-empty list of lines is required for the shuffle format"))]
    ShuffleDemoLogsItemsEmpty,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum OutputFormat {
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
    #[derivative(Default)]
    Json,
}

impl OutputFormat {
    fn generate_line(&self, n: usize) -> String {
        emit!(&DemoLogsEventProcessed);

        match self {
            Self::Shuffle {
                sequence,
                ref lines,
            } => Self::shuffle_generate(*sequence, lines, n),
            Self::ApacheCommon => apache_common_log_line(),
            Self::ApacheError => apache_error_log_line(),
            Self::Syslog => syslog_5424_log_line(),
            Self::BsdSyslog => syslog_3164_log_line(),
            Self::Json => json_log_line(),
        }
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
    pub(self) fn validate(&self) -> Result<(), DemoLogsConfigError> {
        match self {
            Self::Shuffle { lines, .. } => {
                if lines.is_empty() {
                    Err(DemoLogsConfigError::ShuffleDemoLogsItemsEmpty)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
}

impl DemoLogsConfig {
    #[allow(dead_code)] // to make check-component-features pass
    pub fn repeat(lines: Vec<String>, count: usize, interval: f64) -> Self {
        Self {
            count,
            interval,
            format: OutputFormat::Shuffle {
                lines,
                sequence: false,
            },
            framing: default_framing_message_based(),
            decoding: default_decoding(),
        }
    }
}

async fn demo_logs_source(
    interval: f64,
    count: usize,
    format: OutputFormat,
    decoder: codecs::Decoder,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let maybe_interval: Option<f64> = if interval != 0.0 {
        Some(interval)
    } else {
        None
    };

    let mut interval = maybe_interval.map(|i| time::interval(Duration::from_secs_f64(i)));

    for n in 0..count {
        if matches!(futures::poll!(&mut shutdown), Poll::Ready(_)) {
            break;
        }

        if let Some(interval) = &mut interval {
            interval.tick().await;
        }
        emit!(&BytesReceived {
            byte_size: 0,
            protocol: "none",
        });

        let line = format.generate_line(n);

        let mut stream = FramedRead::new(line.as_bytes(), decoder.clone());
        while let Some(next) = stream.next().await {
            match next {
                Ok((events, byte_size)) => {
                    let count = events.len();
                    emit!(&EventsReceived { count, byte_size });
                    let now = Utc::now();

                    let mut events = stream::iter(events).map(|mut event| {
                        let log = event.as_mut_log();

                        log.try_insert(log_schema().source_type_key(), Bytes::from("demo_logs"));
                        log.try_insert(log_schema().timestamp_key(), now);

                        event
                    });
                    out.send_all(&mut events).await.map_err(|error| {
                        emit!(&StreamClosedError { error, count });
                    })?;
                }
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

inventory::submit! {
    SourceDescription::new::<DemoLogsConfig>("demo_logs")
}

inventory::submit! {
    SourceDescription::new::<DemoLogsConfig>("generator")
}

impl_generate_config_from_default!(DemoLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "demo_logs")]
impl SourceConfig for DemoLogsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        self.format.validate()?;
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build()?;
        Ok(Box::pin(demo_logs_source(
            self.interval,
            self.count,
            self.format.clone(),
            decoder,
            cx.shutdown,
            cx.out,
        )))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "demo_logs"
    }
}

// Add a compatibility alias to avoid breaking existing configs
#[derive(Deserialize, Serialize, Debug, Clone)]
struct DemoLogsCompatConfig(DemoLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "generator")]
impl SourceConfig for DemoLogsCompatConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        self.0.build(cx).await
    }

    fn outputs(&self) -> Vec<Output> {
        self.0.outputs()
    }

    fn source_type(&self) -> &'static str {
        self.0.source_type()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use futures::{poll, StreamExt};

    use super::*;
    use crate::{
        config::log_schema, event::Event, shutdown::ShutdownSignal, source_sender::ReceiverStream,
        SourceSender,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DemoLogsConfig>();
    }

    async fn runit(config: &str) -> ReceiverStream<Event> {
        let (tx, rx) = SourceSender::new_test();
        let config: DemoLogsConfig = toml::from_str(config).unwrap();
        let decoder = DecodingConfig::new(default_framing_message_based(), default_decoding())
            .build()
            .unwrap();
        demo_logs_source(
            config.interval,
            config.count,
            config.format,
            decoder,
            ShutdownSignal::noop(),
            tx,
        )
        .await
        .unwrap();
        rx
    }

    #[test]
    fn config_shuffle_lines_not_empty() {
        let empty_lines: Vec<String> = Vec::new();

        let errant_config = DemoLogsConfig {
            format: OutputFormat::Shuffle {
                sequence: false,
                lines: empty_lines,
            },
            ..DemoLogsConfig::default()
        };

        assert_eq!(
            errant_config.format.validate(),
            Err(DemoLogsConfigError::ShuffleDemoLogsItemsEmpty)
        );
    }

    #[tokio::test]
    async fn shuffle_demo_logs_copies_lines() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"format = "shuffle"
               lines = ["one", "two", "three", "four"]
               count = 5"#,
        )
        .await;

        let lines = &["one", "two", "three", "four"];

        for _ in 0..5 {
            let event = match poll!(rx.next()) {
                Poll::Ready(event) => event.unwrap(),
                _ => unreachable!(),
            };
            let log = event.as_log();
            let message = log[&message_key].to_string_lossy();
            assert!(lines.contains(&&*message));
        }

        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn shuffle_demo_logs_limits_count() {
        let mut rx = runit(
            r#"format = "shuffle"
               lines = ["one", "two"]
               count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(poll!(rx.next()).is_ready());
        }
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn shuffle_demo_logs_adds_sequence() {
        let message_key = log_schema().message_key();
        let mut rx = runit(
            r#"format = "shuffle"
               lines = ["one", "two"]
               sequence = true
               count = 5"#,
        )
        .await;

        for n in 0..5 {
            let event = match poll!(rx.next()) {
                Poll::Ready(event) => event.unwrap(),
                _ => unreachable!(),
            };
            let log = event.as_log();
            let message = log[&message_key].to_string_lossy();
            assert!(message.starts_with(&n.to_string()));
        }

        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn shuffle_demo_logs_obeys_interval() {
        let start = Instant::now();
        let mut rx = runit(
            r#"format = "shuffle"
               lines = ["one", "two"]
               count = 3
               interval = 1.0"#,
        )
        .await;

        for _ in 0..3 {
            assert!(poll!(rx.next()).is_ready());
        }
        assert_eq!(poll!(rx.next()), Poll::Ready(None));

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
            assert!(poll!(rx.next()).is_ready());
        }
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn apache_error_format_generates_output() {
        let mut rx = runit(
            r#"format = "apache_error"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(poll!(rx.next()).is_ready());
        }
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn syslog_5424_format_generates_output() {
        let mut rx = runit(
            r#"format = "syslog"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(poll!(rx.next()).is_ready());
        }
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn syslog_3164_format_generates_output() {
        let mut rx = runit(
            r#"format = "bsd_syslog"
            count = 5"#,
        )
        .await;

        for _ in 0..5 {
            assert!(poll!(rx.next()).is_ready());
        }
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
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
            let event = match poll!(rx.next()) {
                Poll::Ready(event) => event.unwrap(),
                _ => unreachable!(),
            };
            let log = event.as_log();
            let message = log[&message_key].to_string_lossy();
            assert!(serde_json::from_str::<serde_json::Value>(&message).is_ok());
        }
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }
}
