use std::{
    pin::Pin,
    task::{Context, Poll},
};

use async_trait::async_trait;
use futures_util::{future::ok, FutureExt, Sink};
use serde::{Deserialize, Serialize};
use vector_core::{
    config::{AcknowledgementsConfig, Input},
    event::Event,
    sink::VectorSink,
};

use crate::{
    config::{SinkConfig, SinkContext, SinkDescription},
    sinks::Healthcheck,
};

/// A test sink that immediately returns an error.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ErrorSinkConfig {
    dummy: Option<String>,
}

impl_generate_config_from_default!(ErrorSinkConfig);

inventory::submit! {
    SinkDescription::new::<ErrorSinkConfig>("error_sink")
}

#[async_trait]
#[typetag::serde(name = "error_sink")]
impl SinkConfig for ErrorSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        Ok((VectorSink::from_event_sink(ErrorSink), ok(()).boxed()))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "error_sink"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
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
