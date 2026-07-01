use std::sync::Arc;

use iggy::prelude::{IggyClient, IggyProducer};
use snafu::ResultExt;

use super::{
    EncodingSnafu, IggyError,
    config::{IggySinkConfig, IggyTowerRequestConfigDefaults},
    request_builder::{IggyRequest, request_builder},
    service::{IggyResponse, IggyService},
};
use crate::sinks::prelude::*;

pub(super) struct IggySink {
    request: TowerRequestConfig<IggyTowerRequestConfigDefaults>,
    transformer: Transformer,
    encoder: Encoder<()>,
    client: Arc<IggyClient>,
    producer: Arc<IggyProducer>,
    batcher_settings: BatcherSettings,
}

impl IggySink {
    pub(super) fn new(
        config: IggySinkConfig,
        client: Arc<IggyClient>,
        producer: Arc<IggyProducer>,
    ) -> Result<Self, IggyError> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build().context(EncodingSnafu)?;
        let batcher_settings = config
            .batch
            .validate()
            .map_err(|_| IggyError::InvalidBatchSettings)?
            .into_batcher_settings()
            .map_err(|_| IggyError::InvalidBatchSettings)?;
        Ok(IggySink {
            request: config.request,
            transformer,
            encoder: Encoder::<()>::new(serializer),
            client,
            producer,
            batcher_settings,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request = self.request.into_settings();
        let client = Arc::clone(&self.client);

        let service = ServiceBuilder::new()
            .settings(request, IggyRetryLogic)
            .service(IggyService {
                producer: Arc::clone(&self.producer),
            });

        let mut encoder = self.encoder.clone();
        let transformer = self.transformer.clone();
        let batcher_settings = self.batcher_settings.as_byte_size_config();

        let result = input
            .batched(batcher_settings)
            .filter_map(|events| {
                futures::future::ready(request_builder(events, &transformer, &mut encoder))
            })
            .into_driver(service)
            .protocol("iggy")
            .run()
            .await;

        use iggy::prelude::Client;
        if let Err(error) = client.disconnect().await {
            warn!(message = "Failed to disconnect Iggy client on sink shutdown.", %error);
        }

        result
    }
}

#[async_trait]
impl StreamSink<Event> for IggySink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[derive(Debug, Clone)]
pub(super) struct IggyRetryLogic;

impl RetryLogic for IggyRetryLogic {
    type Error = IggyError;
    type Request = IggyRequest;
    type Response = IggyResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        use iggy::prelude::IggyError as Sdk;
        let source = match error {
            IggyError::Encoding { .. } | IggyError::InvalidBatchSettings => return false,
            IggyError::Connect { source } | IggyError::Producer { source } => source,
        };
        // Explicit allowlist of transient SDK errors. Defaulting unknown
        // variants to non-retriable means a future SDK version that adds
        // a fatal error (e.g. schema mismatch, payload-too-large) will
        // not silently spin in a retry loop until someone audits the
        // upgrade; any newly-classified transient errors must be added
        // here when bumping the `iggy` dependency.
        matches!(
            source,
            Sdk::Disconnected
                | Sdk::CannotEstablishConnection
                | Sdk::NotConnected
                | Sdk::ConnectionClosed
                | Sdk::StaleClient
                | Sdk::TcpError
                | Sdk::QuicError
                | Sdk::CannotSendMessagesDueToClientDisconnection
                | Sdk::BackgroundWorkerDisconnected
                | Sdk::BackgroundSendTimeout
                | Sdk::TaskTimeout
        )
    }
}
