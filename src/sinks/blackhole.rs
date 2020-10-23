use crate::{
    buffers::Acker,
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    emit,
    internal_events::BlackholeEventReceived,
    sinks::util::StreamSink,
    Event,
};
use async_trait::async_trait;
use futures::{future, stream::BoxStream, FutureExt, StreamExt};
use serde::{Deserialize, Serialize};

pub struct BlackholeSink {
    total_events: usize,
    total_raw_bytes: usize,
    config: BlackholeConfig,
    acker: Acker,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BlackholeConfig {
    pub print_amount: usize,
}

inventory::submit! {
    SinkDescription::new::<BlackholeConfig>("blackhole")
}

impl GenerateConfig for BlackholeConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self { print_amount: 1000 }).unwrap()
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
            total_events: 0,
            total_raw_bytes: 0,
            acker,
        }
    }
}

#[async_trait]
impl StreamSink for BlackholeSink {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            let message_len = std::mem::size_of_val(&event);

            self.total_events += 1;
            self.total_raw_bytes += message_len;

            emit!(BlackholeEventReceived {
                byte_size: message_len
            });

            if self.total_events % self.config.print_amount == 0 {
                info!({
                    events = self.total_events,
                    raw_bytes_collected = self.total_raw_bytes
                }, "Total events collected");
            }

            self.acker.ack(1);
        }
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
        let config = BlackholeConfig { print_amount: 10 };
        let mut sink = BlackholeSink::new(config, Acker::Null);

        let (_input_lines, events) = random_events_with_stream(100, 10);
        let _ = sink.run(Box::pin(events)).await.unwrap();
    }
}
