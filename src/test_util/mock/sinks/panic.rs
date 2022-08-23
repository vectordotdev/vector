use std::{
    pin::Pin,
    task::{Context, Poll},
};

use async_trait::async_trait;
use futures_util::{future::ok, FutureExt, Sink};
use vector_config::configurable_component;
use vector_core::{
    config::{AcknowledgementsConfig, Input},
    event::Event,
    sink::VectorSink,
};

use crate::{
    config::{SinkConfig, SinkContext},
    sinks::Healthcheck,
};

/// Configuration for the `test_panic` sink.
#[configurable_component(sink)]
#[derive(Clone, Debug, Default)]
pub struct PanicSinkConfig {
    /// Dummy field used for generating unique configurations to trigger reloads.
    dummy: Option<String>,
}

impl_generate_config_from_default!(PanicSinkConfig);

#[async_trait]
#[typetag::serde(name = "test_panic")]
impl SinkConfig for PanicSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        Ok((VectorSink::from_event_sink(PanicSink), ok(()).boxed()))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "test_panic"
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

struct PanicSink;

impl Sink<Event> for PanicSink {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        panic!()
    }

    fn start_send(self: Pin<&mut Self>, _item: Event) -> Result<(), Self::Error> {
        panic!()
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        panic!()
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        panic!()
    }
}
