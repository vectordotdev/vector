use azure_messaging_eventhubs::ProducerClient;
use bytes::BytesMut;
use tokio_util::codec::Encoder as _;

use super::config::AzureEventHubsSinkConfig;
use crate::{sinks::prelude::*, sources::azure_event_hubs::build_credential};

pub struct AzureEventHubsSink {
    producer: ProducerClient,
    transformer: Transformer,
    encoder: crate::codecs::Encoder<()>,
}

impl AzureEventHubsSink {
    pub async fn new(config: &AzureEventHubsSinkConfig) -> crate::Result<Self> {
        let (namespace, event_hub_name, credential) = build_credential(
            config.connection_string.as_ref(),
            config.namespace.as_deref(),
            config.event_hub_name.as_deref(),
        )?;

        let producer = ProducerClient::builder()
            .open(&namespace, &event_hub_name, credential)
            .await
            .map_err(|e| format!("Failed to create Event Hubs producer: {e}"))?;

        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = crate::codecs::Encoder::<()>::new(serializer);

        Ok(Self {
            producer,
            transformer,
            encoder,
        })
    }

    async fn run_inner(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(mut event) = input.next().await {
            let _byte_size = event.estimated_json_encoded_size_of();
            let finalizers = event.take_finalizers();

            self.transformer.transform(&mut event);

            let mut body = BytesMut::new();
            if self.encoder.encode(event, &mut body).is_err() {
                error!(message = "Failed to encode event for Event Hubs.");
                finalizers.update_status(EventStatus::Errored);
                continue;
            }

            let event_data = azure_messaging_eventhubs::models::EventData::builder()
                .with_body(body.freeze().to_vec())
                .build();

            match self.producer.send_event(event_data, None).await {
                Ok(_) => {
                    finalizers.update_status(EventStatus::Delivered);
                }
                Err(e) => {
                    error!(message = "Failed to send event to Event Hubs.", error = %e);
                    finalizers.update_status(EventStatus::Errored);
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl StreamSink<Event> for AzureEventHubsSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
