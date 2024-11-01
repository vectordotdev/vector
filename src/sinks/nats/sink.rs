use std::sync::Arc;

use snafu::ResultExt;

use crate::sinks::prelude::*;

use super::{
    config::{NatsPublisher, NatsSinkConfig, NatsTowerRequestConfigDefaults},
    request_builder::{NatsEncoder, NatsRequestBuilder},
    service::{NatsResponse, NatsService},
    EncodingSnafu, NatsError,
};

pub(super) struct NatsEvent {
    pub(super) event: Event,
    pub(super) subject: String,
}

pub(super) struct NatsSink {
    request: TowerRequestConfig<NatsTowerRequestConfigDefaults>,
    transformer: Transformer,
    encoder: Encoder<()>,
    publisher: Arc<NatsPublisher>,
    subject: Template,
}

impl NatsSink {
    fn make_nats_event(&self, event: Event) -> Option<NatsEvent> {
        let subject = self
            .subject
            .render_string(&event)
            .map_err(|missing_keys| {
                emit!(TemplateRenderingError {
                    error: missing_keys,
                    field: Some("subject"),
                    drop_event: true,
                });
            })
            .ok()?;

        Some(NatsEvent { event, subject })
    }

    pub(super) async fn new(config: NatsSinkConfig) -> Result<Self, NatsError> {
        let publisher = Arc::new(config.publisher().await?);
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build().context(EncodingSnafu)?;
        let encoder = Encoder::<()>::new(serializer);
        let request = config.request;
        let subject = config.subject;

        Ok(NatsSink {
            request,
            transformer,
            encoder,
            publisher,
            subject,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request = self.request.into_settings();

        let request_builder = NatsRequestBuilder {
            encoder: NatsEncoder {
                encoder: self.encoder.clone(),
                transformer: self.transformer.clone(),
            },
        };

        let service = ServiceBuilder::new()
            .settings(request, NatsRetryLogic)
            .service(NatsService {
                publisher: Arc::clone(&self.publisher),
            });

        input
            .filter_map(|event| std::future::ready(self.make_nats_event(event)))
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build NATS request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(service)
            .protocol("nats")
            .run()
            .await
    }
}

#[async_trait]
impl StreamSink<Event> for NatsSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[derive(Debug, Clone)]
pub(super) struct NatsRetryLogic;

impl RetryLogic for NatsRetryLogic {
    type Error = NatsError;
    type Response = NatsResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }
}
