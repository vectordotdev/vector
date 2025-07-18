use std::future;

use redis::{aio::ConnectionManager, RedisError};

use crate::sinks::{prelude::*, util::retries::RetryAction};

use super::{
    config::{DataTypeConfig, RedisSinkConfig, RedisTowerRequestConfigDefaults},
    request_builder::request_builder,
    service::{RedisResponse, RedisService},
    RedisEvent,
};

pub(super) struct RedisSink {
    request: TowerRequestConfig<RedisTowerRequestConfigDefaults>,
    encoder: crate::codecs::Encoder<()>,
    transformer: crate::codecs::Transformer,
    conn: ConnectionManager,
    data_type: super::DataType,
    key: Template,
    batcher_settings: BatcherSettings,
}

impl RedisSink {
    pub(super) fn new(config: &RedisSinkConfig, conn: ConnectionManager) -> crate::Result<Self> {
        let method = config.list_option.map(|option| option.method);
        let data_type = match config.data_type {
            DataTypeConfig::Channel => super::DataType::Channel,
            DataTypeConfig::List => super::DataType::List(method.unwrap_or_default()),
        };

        let batcher_settings = config.batch.validate()?.into_batcher_settings()?;
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let key = config.key.clone();
        let request = config.request;

        Ok(RedisSink {
            request,
            batcher_settings,
            transformer,
            encoder,
            conn,
            data_type,
            key,
        })
    }

    /// Transforms an event into a `Redis` event by rendering the template field used to
    /// determine the key.
    /// Returns `None` if there is an error whilst rendering. An error event is also emitted.
    fn make_redis_event(&self, event: Event) -> Option<RedisEvent> {
        let key = self
            .key
            .render_string(&event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("key"),
                    drop_event: true,
                });
            })
            .ok()?;

        Some(RedisEvent { event, key })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request = self.request.into_settings();

        let service = RedisService {
            conn: self.conn.clone(),
            data_type: self.data_type,
        };

        let service = ServiceBuilder::new()
            .settings(request, RedisRetryLogic)
            .service(service);

        let mut encoder = self.encoder.clone();
        let transformer = self.transformer.clone();
        let batcher_settings = self.batcher_settings.as_byte_size_config();

        input
            .filter_map(|event| future::ready(self.make_redis_event(event)))
            .batched(batcher_settings)
            .map(|events| request_builder(events, &transformer, &mut encoder))
            .into_driver(service)
            .protocol("redis")
            .run()
            .await
    }
}

#[async_trait]
impl StreamSink<Event> for RedisSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[derive(Debug, Clone)]
pub(super) struct RedisRetryLogic;

impl RetryLogic for RedisRetryLogic {
    type Error = RedisError;
    type Response = RedisResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        if response.is_successful() {
            RetryAction::Successful
        } else {
            RetryAction::Retry("Sending data to redis failed.".into())
        }
    }
}
