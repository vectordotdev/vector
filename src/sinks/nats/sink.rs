use std::sync::Arc;

use snafu::ResultExt;

use crate::sinks::prelude::*;

use super::{
    config::NatsSinkConfig,
    request_builder::{NatsEncoder, NatsRequestBuilder},
    service::NatsService,
    EncodingSnafu, NatsError, SubjectTemplateSnafu,
};

pub(super) struct NatsEvent {
    pub(super) event: Event,
    pub(super) subject: String,
}

pub(super) struct NatsSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    connection: Arc<async_nats::Client>,
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
        let connection = Arc::new(config.connect().await?);
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build().context(EncodingSnafu)?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(NatsSink {
            connection,
            transformer,
            encoder,
            subject: Template::try_from(config.subject).context(SubjectTemplateSnafu)?,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = NatsRequestBuilder {
            encoder: NatsEncoder {
                encoder: self.encoder.clone(),
                transformer: self.transformer.clone(),
            },
        };

        let service = ServiceBuilder::new().service(NatsService {
            connection: self.connection.clone(),
        });

        input
            .filter_map(|event| std::future::ready(self.make_nats_event(event)))
            .request_builder(None, request_builder)
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
        // let bytes_sent = register!(BytesSent::from(Protocol::TCP));
        // let events_sent = register!(EventsSent::from(Output(None)));

        // while let Some(mut event) = input.next().await {
        //     let finalizers = event.take_finalizers();

        //     let subject = match self.subject.render_string(&event) {
        //         Ok(subject) => subject,
        //         Err(error) => {
        //             emit!(TemplateRenderingError {
        //                 error,
        //                 field: Some("subject"),
        //                 drop_event: true,
        //             });
        //             finalizers.update_status(EventStatus::Rejected);
        //             continue;
        //         }
        //     };

        //     self.transformer.transform(&mut event);

        //     let event_byte_size = event.estimated_json_encoded_size_of();

        //     let mut bytes = BytesMut::new();
        //     if self.encoder.encode(event, &mut bytes).is_err() {
        //         // Error is handled by `Encoder`.
        //         finalizers.update_status(EventStatus::Rejected);
        //         continue;
        //     }
        // }

        // Ok(())
    }
}
