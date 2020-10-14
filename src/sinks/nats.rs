use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    emit,
    event::Event,
    internal_events::NatsEventSent,
    sinks::util::StreamSink,
};
use async_trait::async_trait;
use futures::{future, stream::BoxStream, FutureExt, StreamExt};
use nats;
use serde::{Deserialize, Serialize};

/**
 * Code dealing with the SinkConfig struct.
 *
 * DEV: Start with the bare minimum for now.
 */

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NatsSinkConfig {
    url: String,
    subject: String,
}

inventory::submit! {
    SinkDescription::new::<NatsSinkConfig>("nats")
}

impl GenerateConfig for NatsSinkConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SinkConfig for NatsSinkConfig {
    async fn build(
        &self,
        _cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = NatsSink::new(self.clone());
        let healthcheck = future::ok(()).boxed();

        Ok((super::VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "nats"
    }
}

/**
 * Code dealing with the Sink struct.
 */

#[derive(Clone)]
pub struct NatsSink {
    nc: nats::Connection,
    subject: String,
}

impl NatsSink {
    fn new(config: NatsSinkConfig) -> Self {
        let options = nats::Options::new();
        let nc = options.connect(&config.url).unwrap();

        Self {
            nc: nc,
            subject: config.subject.clone(),
        }
    }
}

#[async_trait]
impl StreamSink for NatsSink {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            match event {
                Event::Log(log) => {
                    let body = log
                        .get(crate::config::log_schema().message_key())
                        .map(|v| v.to_string_lossy())
                        .unwrap_or_else(|| "".into());

                    let message_len = body.len();

                    match self.nc.publish(&self.subject, body) {
                        Ok(_) => {
                            emit!(NatsEventSent {
                                byte_size: message_len,
                            });
                        }
                        _ => {
                            // DEV: Code path for handling unsuccessful
                            //      publications. Future work to buffer unsent
                            //      messages might go here.
                        }
                    }
                }
                Event::Metric(_metric) => {}
            }
        }

        Ok(())
    }
}

#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::test_util::{random_lines_with_stream, random_string, trace_init};
    use std::{sync::{Arc, Mutex}, time::Duration};

    #[tokio::test]
    async fn nats_happy() {
        // Publish `N` messages to NATS.
        //
        // Observe with a second subscriber that at least some of the messages
        // were received.
        //
        // NATS operates with at_most_once delivery semantics
        // - https://docs.nats.io/faq#does-nats-guarantee-message-delivery
        //
        // All messages should be accountable in the local integration test
        // case, but, to prevent flakiness initially, validate the basic
        // acceptable outcome.

        trace_init();

        let subject = format!("test-{}", random_string(10));

        let cnf = NatsSinkConfig {
            url: "nats://127.0.0.1:4222".to_owned(),
            subject: subject.clone(),
        };

        // Establish the consumer subscription.
        let consumer = NatsSink::new(cnf.clone());
        let num_recv_events: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
        let sub = consumer.nc.subscribe(&subject).unwrap();

        // Publish events.
        let mut sink = NatsSink::new(cnf.clone());
        let num_events = 1_000;
        let (_input_lines, events) = random_lines_with_stream(100, num_events);

        let _ = sink.run(Box::pin(events)).await.unwrap();

        // Observe that there are delivered events.
        for _msg in sub.timeout_iter(Duration::from_secs(3)) {
            *num_recv_events.lock().unwrap() += 1;
        }
        assert!(*num_recv_events.lock().unwrap() > 0);
    }
}
