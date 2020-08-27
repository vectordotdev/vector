use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    emit,
    event::{self, Event},
    internal_events::NatsEventSent,
    sinks::util::StreamSink,
};
use async_trait::async_trait;
use futures::pin_mut;
use futures::stream::{Stream, StreamExt};
use futures01::future;
use nats;
use serde::{Deserialize, Serialize};

use super::streaming_sink::{self, StreamingSink};

/**
 * Code dealing with the SinkConfig struct.
 *
 * DEV: Start with the bare minimum for now.
 */

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NatsSinkConfig {
    server: String,
    subject: String,
}

inventory::submit! {
    SinkDescription::new_without_default::<NatsSinkConfig>("nats")
}

#[typetag::serde(name = "nats")]
impl SinkConfig for NatsSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = NatsSink::new(self.clone());
        let sink = streaming_sink::compat::adapt_to_topology(sink);
        let sink = StreamSink::new(sink, cx.acker());
        let sink = Box::new(sink);

        let healthcheck = Box::new(future::ok(()));

        Ok((sink, healthcheck))
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
        let nc = options.connect(&config.server).unwrap();

        Self {
            nc: nc,
            subject: config.subject.clone(),
        }
    }
}

#[async_trait]
impl StreamingSink for NatsSink {
    async fn run(
        &mut self,
        input: impl Stream<Item = Event> + Send + Sync + 'static,
    ) -> crate::Result<()> {
        pin_mut!(input);
        while let Some(event) = input.next().await {
            match event {
                Event::Log(log) => {
                    let body = log
                        .get(&event::log_schema().message_key())
                        .map(|v| v.to_string_lossy())
                        .unwrap_or_else(|| "".into());

                    let message_len = body.len();

                    self.nc.publish(&self.subject, body)?;

                    emit!(NatsEventSent {
                        byte_size: message_len,
                    });

                },
                Event::Metric(_metric) => {
                }
            }
        }

        Ok(())
    }
}
