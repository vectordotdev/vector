use crate::{
    buffers::Acker,
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    emit,
    internal_events::BlackholeEventReceived,
    sinks::util::StreamSink,
};
use async_trait::async_trait;
use futures::{future, stream::BoxStream, FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    select,
    sync::watch,
    time::{interval, sleep_until},
};
use vector_core::event::Event;
use vector_core::ByteSizeOf;

pub struct BlackholeSink {
    total_events: Arc<AtomicUsize>,
    total_raw_bytes: Arc<AtomicUsize>,
    config: BlackholeConfig,
    acker: Acker,
    last: Option<Instant>,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct BlackholeConfig {
    #[derivative(Default(value = "1"))]
    #[serde(default = "default_print_interval_secs")]
    pub print_interval_secs: u64,
    pub rate: Option<usize>,
}

const fn default_print_interval_secs() -> u64 {
    1
}

inventory::submit! {
    SinkDescription::new::<BlackholeConfig>("blackhole")
}

impl GenerateConfig for BlackholeConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(&Self::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "blackhole")]
impl SinkConfig for BlackholeConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = BlackholeSink::new(self.clone(), cx.acker());
        let healthcheck = future::ok(()).boxed();

        Ok((super::VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "blackhole"
    }
}

impl BlackholeSink {
    pub fn new(config: BlackholeConfig, acker: Acker) -> Self {
        BlackholeSink {
            config,
            total_events: Arc::new(AtomicUsize::new(0)),
            total_raw_bytes: Arc::new(AtomicUsize::new(0)),
            acker,
            last: None,
        }
    }
}

#[async_trait]
impl StreamSink for BlackholeSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // Spin up a task that does the periodic reporting.  This is decoupled from the main sink so
        // that rate limiting support can be added more simply without having to interleave it with
        // the printing.
        let total_events = Arc::clone(&self.total_events);
        let total_raw_bytes = Arc::clone(&self.total_raw_bytes);
        let interval_dur = Duration::from_secs(self.config.print_interval_secs);
        let (shutdown, mut tripwire) = watch::channel(());

        tokio::spawn(async move {
            let mut print_interval = interval(interval_dur);
            loop {
                select! {
                    _ = print_interval.tick() => {
                        info!({
                            events = total_events.load(Ordering::Relaxed),
                            raw_bytes_collected = total_raw_bytes.load(Ordering::Relaxed),
                        }, "Total events collected");
                    },
                    _ = tripwire.changed() => break,
                }
            }

            info!({
                events = total_events.load(Ordering::Relaxed),
                raw_bytes_collected = total_raw_bytes.load(Ordering::Relaxed)
            }, "Total events collected");
        });

        let mut chunks = input.ready_chunks(1024);
        while let Some(events) = chunks.next().await {
            if let Some(rate) = self.config.rate {
                let factor: f32 = 1.0 / rate as f32;
                let secs: f32 = factor * (events.len() as f32);
                let until = self.last.unwrap_or_else(Instant::now) + Duration::from_secs_f32(secs);
                sleep_until(until.into()).await;
                self.last = Some(until);
            }

            let message_len = events.size_of();

            let _ = self.total_events.fetch_add(events.len(), Ordering::AcqRel);
            let _ = self
                .total_raw_bytes
                .fetch_add(message_len, Ordering::AcqRel);

            emit!(&BlackholeEventReceived {
                byte_size: message_len
            });

            self.acker.ack(events.len());
        }

        // Notify the reporting task to shutdown.
        let _ = shutdown.send(());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::random_events_with_stream;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<BlackholeConfig>();
    }

    #[tokio::test]
    async fn blackhole() {
        let config = BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        };
        let sink = Box::new(BlackholeSink::new(config, Acker::Null));

        let (_input_lines, events) = random_events_with_stream(100, 10, None);
        let _ = sink.run(Box::pin(events)).await.unwrap();
    }
}
