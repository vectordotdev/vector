use super::{
    config::AzureEventHubsSinkConfig,
    request_builder::AzureEventHubsRequestBuilder,
    service::AzureEventHubsService,
};
use crate::{sinks::prelude::*, sources::azure_event_hubs::build_credential};

pub struct AzureEventHubsSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    service: AzureEventHubsService,
}

impl AzureEventHubsSink {
    pub async fn new(config: &AzureEventHubsSinkConfig) -> crate::Result<Self> {
        let (namespace, event_hub_name, credential, custom_endpoint) = build_credential(
            config.connection_string.as_ref(),
            config.namespace.as_deref(),
            config.event_hub_name.as_deref(),
        )?;

        let mut builder = azure_messaging_eventhubs::ProducerClient::builder();
        if let Some(endpoint) = custom_endpoint {
            builder = builder.with_custom_endpoint(endpoint);
        }
        let producer = builder
            .open(&namespace, &event_hub_name, credential)
            .await
            .map_err(|e| format!("Failed to create Event Hubs producer: {e}"))?;

        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        let request_limits = config.request.into_settings();
        let max_in_flight = request_limits.concurrency.unwrap_or(100);

        Ok(Self {
            transformer,
            encoder,
            service: AzureEventHubsService::new(producer, max_in_flight),
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = AzureEventHubsRequestBuilder {
            encoder: (self.transformer, self.encoder),
        };

        input
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| async {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .protocol("amqp")
            .run()
            .await
    }
}

#[async_trait]
impl StreamSink<Event> for AzureEventHubsSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
