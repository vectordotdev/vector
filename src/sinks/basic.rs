#![allow(unused_imports)]
use super::Healthcheck;
use crate::config::{GenerateConfig, SinkConfig, SinkContext};
use futures::{stream::BoxStream, StreamExt};
use vector_common::{
    finalization::{EventStatus, Finalizable},
    internal_event::{BytesSent, EventsSent},
};
use vector_config::configurable_component;
use vector_core::{
    config::{AcknowledgementsConfig, Input},
    event::Event,
    sink::{StreamSink, VectorSink},
    EstimatedJsonEncodedSizeOf,
};

#[configurable_component(sink("basic"))]
#[derive(Clone, Debug)]
/// A basic sink that dumps its output to stdout.
pub struct BasicConfig {
    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for BasicConfig {
    fn generate_config() -> toml::Value {
        toml::from_str("").unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for BasicConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let healthcheck = Box::pin(async move { Ok(()) });
        let sink = VectorSink::from_event_streamsink(BasicSink);

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct BasicSink;

#[async_trait::async_trait]
impl StreamSink<Event> for BasicSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

impl BasicSink {
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            let bytes = format!("{:#?}", event);
            println!("{}", bytes);

            emit!(BytesSent {
                byte_size: bytes.len(),
                protocol: "none".into()
            });

            let event_byte_size = event.estimated_json_encoded_size_of();
            emit!(EventsSent {
                count: 1,
                byte_size: event_byte_size,
                output: None,
            })
        }

        Ok(())
    }
}
