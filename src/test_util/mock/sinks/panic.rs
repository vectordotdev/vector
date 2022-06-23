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

/// A test sink that immediately panics.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PanicSinkConfig {
    dummy: Option<String>,
}

impl_generate_config_from_default!(PanicSinkConfig);

inventory::submit! {
    SinkDescription::new::<PanicSinkConfig>("panic_sink")
}

#[async_trait]
#[typetag::serde(name = "panic_sink")]
impl SinkConfig for PanicSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        Ok((VectorSink::from_event_sink(PanicSink), ok(()).boxed()))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "panic_sink"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
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
