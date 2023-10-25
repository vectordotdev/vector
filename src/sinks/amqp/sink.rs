//! The sink for the `AMQP` sink that wires together the main stream that takes the
//! event and sends it to `AMQP`.
use crate::sinks::prelude::*;
use lapin::{options::ConfirmSelectOptions, BasicProperties};
use serde::Serialize;
use std::sync::Arc;

use super::{
    config::{AmqpPropertiesConfig, AmqpSinkConfig},
    encoder::AmqpEncoder,
    request_builder::AmqpRequestBuilder,
    service::AmqpService,
    BuildError,
};

/// Stores the event together with the rendered exchange and routing_key values.
/// This is passed into the `RequestBuilder` which then splits it out into the event
/// and metadata containing the exchange and routing_key.
/// This event needs to be created prior to building the request so we can filter out
/// any events that error whilst rendering the templates.
#[derive(Serialize)]
pub(super) struct AmqpEvent {
    pub(super) event: Event,
    pub(super) exchange: String,
    pub(super) routing_key: String,
    pub(super) properties: BasicProperties,
}

pub(super) struct AmqpSink {
    pub(super) channel: Arc<lapin::Channel>,
    exchange: Template,
    routing_key: Option<Template>,
    properties: Option<AmqpPropertiesConfig>,
    transformer: Transformer,
    encoder: crate::codecs::Encoder<()>,
}

impl AmqpSink {
    pub(super) async fn new(config: AmqpSinkConfig) -> crate::Result<Self> {
        let (_, channel) = config
            .connection
            .connect()
            .await
            .map_err(|e| BuildError::AmqpCreateFailed { source: e })?;

        // Enable confirmations on the channel.
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await
            .map_err(|e| BuildError::AmqpCreateFailed {
                source: Box::new(e),
            })?;

        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = crate::codecs::Encoder::<()>::new(serializer);

        Ok(AmqpSink {
            channel: Arc::new(channel),
            exchange: config.exchange,
            routing_key: config.routing_key,
            properties: config.properties,
            transformer,
            encoder,
        })
    }

    /// Transforms an event into an `AMQP` event by rendering the required template fields.
    /// Returns None if there is an error whilst rendering.
    fn make_amqp_event(&self, event: Event) -> Option<AmqpEvent> {
        let exchange = self
            .exchange
            .render_string(&event)
            .map_err(|missing_keys| {
                emit!(TemplateRenderingError {
                    error: missing_keys,
                    field: Some("exchange"),
                    drop_event: true,
                })
            })
            .ok()?;

        let routing_key = match &self.routing_key {
            None => String::new(),
            Some(key) => key
                .render_string(&event)
                .map_err(|missing_keys| {
                    emit!(TemplateRenderingError {
                        error: missing_keys,
                        field: Some("routing_key"),
                        drop_event: true,
                    })
                })
                .ok()?,
        };

        let properties = match &self.properties {
            None => BasicProperties::default(),
            Some(prop) => prop.build(),
        };

        Some(AmqpEvent {
            event,
            exchange,
            routing_key,
            properties,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = AmqpRequestBuilder {
            encoder: AmqpEncoder {
                encoder: self.encoder.clone(),
                transformer: self.transformer.clone(),
            },
        };
        let service = ServiceBuilder::new().service(AmqpService {
            channel: Arc::clone(&self.channel),
        });

        input
            .filter_map(|event| std::future::ready(self.make_amqp_event(event)))
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build AMQP request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(service)
            .protocol("amqp_0_9_1")
            .run()
            .await
    }
}

#[async_trait]
impl StreamSink<Event> for AmqpSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
