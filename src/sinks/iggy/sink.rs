use std::sync::Arc;

use iggy::prelude::IggyProducer;
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
    producer: Arc<IggyProducer>,
    batcher_settings: BatcherSettings,
}

impl IggySink {
    pub(super) fn new(
        config: IggySinkConfig,
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
            producer,
            batcher_settings,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request = self.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(request, IggyRetryLogic)
            .service(IggyService {
                producer: Arc::clone(&self.producer),
            });

        let mut encoder = self.encoder.clone();
        let transformer = self.transformer.clone();
        let batcher_settings = self.batcher_settings.as_byte_size_config();

        input
            .batched(batcher_settings)
            .map(|events| request_builder(events, &transformer, &mut encoder))
            .into_driver(service)
            .protocol("iggy")
            .run()
            .await
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
        !matches!(
            source,
            Sdk::Unauthenticated
                | Sdk::Unauthorized
                | Sdk::InvalidCredentials
                | Sdk::InvalidUsername
                | Sdk::InvalidPassword
                | Sdk::InvalidPersonalAccessToken
                | Sdk::PersonalAccessTokenExpired(..)
                | Sdk::AccessTokenMissing
                | Sdk::InvalidAccessToken
                | Sdk::JwtMissing
                | Sdk::StreamIdNotFound(..)
                | Sdk::StreamNameNotFound(..)
                | Sdk::TopicIdNotFound(..)
                | Sdk::TopicNameNotFound(..)
                | Sdk::PartitionNotFound(..)
                | Sdk::InvalidStreamName
                | Sdk::InvalidStreamId
                | Sdk::InvalidTopicName
                | Sdk::InvalidTopicId
                | Sdk::InvalidConfiguration
                | Sdk::InvalidCommand
                | Sdk::InvalidFormat
                | Sdk::FeatureUnavailable
        )
    }
}
