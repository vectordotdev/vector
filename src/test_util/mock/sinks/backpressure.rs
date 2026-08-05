use async_trait::async_trait;
use futures::{FutureExt, StreamExt, stream::BoxStream};
use vector_lib::configurable::configurable_component;

use crate::{
    config::{
        AcknowledgementsConfig, DynValidatedSink, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    event::Event,
    sinks::{Healthcheck, VectorSink, util::StreamSink},
};

#[derive(Debug)]
struct BackpressureSink {
    num_to_consume: usize,
}

#[async_trait]
impl StreamSink<Event> for BackpressureSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let _num_taken = input.take(self.num_to_consume).count().await;
        futures::future::pending::<()>().await;
        Ok(())
    }
}

/// Configuration for the `test_backpressure` sink.
#[configurable_component(sink("test_backpressure", "Test (backpressure)."))]
#[derive(Clone, Debug, Default)]
pub struct BackpressureSinkConfig {
    /// Number of events to consume before stopping.
    pub num_to_consume: usize,
}

impl_generate_config_from_default!(BackpressureSinkConfig);

#[async_trait]
#[typetag::serde(name = "test_backpressure")]
impl SinkConfig for BackpressureSinkConfig {
    fn as_dyn_validated(&self) -> Option<&dyn DynValidatedSink> {
        Some(self)
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

#[async_trait]
impl ValidatedSink for BackpressureSinkConfig {
    type Validated = Self;

    fn validate(&self) -> crate::Result<Self::Validated> {
        Ok(self.clone())
    }

    async fn build(
        &self,
        validated: &Self::Validated,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = BackpressureSink {
            num_to_consume: validated.num_to_consume,
        };
        let healthcheck = futures::future::ok(()).boxed();
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}
