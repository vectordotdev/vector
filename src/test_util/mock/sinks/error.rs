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

/// Configuration for the `test_error` sink.
#[configurable_component(sink)]
#[derive(Clone, Debug, Default)]
pub struct ErrorSinkConfig {
    /// Dummy field used for generating unique configurations to trigger reloads.
    dummy: Option<String>,
}

impl_generate_config_from_default!(ErrorSinkConfig);

#[async_trait]
#[typetag::serde(name = "test_error")]
impl SinkConfig for ErrorSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        Ok((VectorSink::from_event_sink(ErrorSink), ok(()).boxed()))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "test_error"
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

struct ErrorSink;

impl Sink<Event> for ErrorSink {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(()))
    }

    fn start_send(self: Pin<&mut Self>, _item: Event) -> Result<(), Self::Error> {
        Err(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(()))
    }
}
