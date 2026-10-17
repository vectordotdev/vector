use crate::dummy_service::{DummyLogsEncoder, DummyRequestBuilder, DummyService};

use derivative::Derivative;
use futures_util::{future, FutureExt, StreamExt};

use tower::ServiceBuilder;

use vector::config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext};
use vector::event::Event;
use vector::sinks::prelude::{
    default_request_builder_concurrency_limit, BatchConfig, BatcherSettings, BoxStream,
};

use vector::sinks::util::{RealtimeEventBasedDefaultBatchSettings, SinkBuilderExt};
use vector::sinks::{Healthcheck, VectorSink};
use vector_lib::configurable::configurable_component;
use vector_lib::impl_generate_config_from_default;
use vector_lib::sink::StreamSink;
use vector_lib::Result as VectorResult;

/// Configuration for the `unit_test` sink.
#[configurable_component(sink("dummy_sink", "Unit test."))]
#[derive(Clone, Default, Derivative)]
#[derivative(Debug)]
pub struct DummySinkConfig {
    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeEventBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "vector::serde::bool_or_struct",
        skip_serializing_if = "vector::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl_generate_config_from_default!(DummySinkConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "dummy_sink")]
impl SinkConfig for DummySinkConfig {
    async fn build(&self, _cx: SinkContext) -> VectorResult<(VectorSink, Healthcheck)> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let dummy_service = DummyService {};

        let service = ServiceBuilder::new().service(dummy_service);

        let sink = DummySink::new(batch_settings, service);
        let healthcheck = future::ok(()).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

pub struct DummySink {
    batch_settings: BatcherSettings,
    service: DummyService,
}

impl DummySink {
    pub fn new(batch_settings: BatcherSettings, service: DummyService) -> Self {
        Self {
            batch_settings,
            service,
        }
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for DummySink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = DummyRequestBuilder {
            encoder: DummyLogsEncoder {},
        };
        input
            .batched(self.batch_settings.as_byte_size_config())
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .map(|r| r.unwrap())
            .into_driver(self.service)
            .run()
            .await
    }
}
