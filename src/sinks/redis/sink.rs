use std::{
    future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use redis::{
    aio::ConnectionManager,
    sentinel::{Sentinel, SentinelNodeConnectionInfo},
    RedisResult,
};
use tokio::sync::RwLock;

use crate::sinks::{prelude::*, redis::RedisSinkError, util::retries::RetryAction};

use super::{
    config::{DataTypeConfig, RedisSinkConfig, RedisTowerRequestConfigDefaults},
    request_builder::request_builder,
    service::{RedisResponse, RedisService},
    RedisEvent,
};

#[derive(Clone)]
pub(super) enum RedisConnection {
    Direct(ConnectionManager),
    Sentinel {
        // Tokio's RwLock was used instead of std since we need to hold it
        // across await points in [`Self::get_connection_manager`]
        sentinel: Arc<RwLock<Sentinel>>,
        service_name: String,
        node_connection_info: SentinelNodeConnectionInfo,
        last_conn: Arc<RwLock<ConnectionManager>>,
        // Flag needed as we cannot call async methods to fix the redis
        // connection with sentinel from the sync `RetryLogic::is_retriable_error`.
        needs_fix: Arc<AtomicBool>,
    },
}

impl RedisConnection {
    pub(super) fn new_direct(conn: ConnectionManager) -> Self {
        Self::Direct(conn)
    }

    async fn sentinel_connection_manager(
        sentinel: &mut Sentinel,
        service_name: &str,
        node_connection_info: &SentinelNodeConnectionInfo,
    ) -> RedisResult<ConnectionManager> {
        let master = sentinel
            .async_master_for(service_name, Some(node_connection_info))
            .await?;
        master.get_connection_manager().await
    }

    pub(super) async fn new_sentinel(
        mut sentinel: Sentinel,
        service_name: String,
        node_connection_info: SentinelNodeConnectionInfo,
    ) -> RedisResult<Self> {
        let conn = Self::sentinel_connection_manager(
            &mut sentinel,
            service_name.as_str(),
            &node_connection_info,
        )
        .await?;

        Ok(Self::Sentinel {
            sentinel: Arc::new(RwLock::new(sentinel)),
            service_name,
            node_connection_info,
            last_conn: Arc::new(RwLock::new(conn)),
            needs_fix: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(super) async fn get_connection_manager(&self) -> RedisResult<ConnectionManager> {
        match self {
            Self::Direct(conn) => Ok(conn.clone()),
            Self::Sentinel {
                sentinel,
                service_name,
                node_connection_info,
                last_conn,
                needs_fix,
            } => {
                // Get fix flag and reset to false, return early if no fix is needed
                if !needs_fix.swap(false, Ordering::SeqCst) {
                    return Ok(last_conn.read().await.clone());
                }

                let mut sentinel = sentinel.write().await;
                let mngr = Self::sentinel_connection_manager(
                    &mut sentinel,
                    service_name.as_str(),
                    node_connection_info,
                )
                .await?;

                let mut last_conn = last_conn.write().await;
                *last_conn = mngr.clone();

                Ok(mngr)
            }
        }
    }

    pub(super) fn signal_needs_fix(&self) {
        if let Self::Sentinel { needs_fix, .. } = self {
            needs_fix.store(true, Ordering::SeqCst);
        }
    }
}

pub(super) struct RedisSink {
    request: TowerRequestConfig<RedisTowerRequestConfigDefaults>,
    encoder: crate::codecs::Encoder<()>,
    transformer: crate::codecs::Transformer,
    conn: RedisConnection,
    data_type: super::DataType,
    key: Template,
    batcher_settings: BatcherSettings,
}

impl RedisSink {
    pub(super) fn new(config: &RedisSinkConfig, conn: RedisConnection) -> crate::Result<Self> {
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
            .settings(
                request,
                RedisRetryLogic {
                    conn: self.conn.clone(),
                },
            )
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

#[derive(Clone)]
pub(super) struct RedisRetryLogic {
    conn: RedisConnection,
}

impl RetryLogic for RedisRetryLogic {
    type Error = RedisSinkError;
    type Response = RedisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RedisSinkError::SendError { source }
                if matches!(
                    source.kind(),
                    redis::ErrorKind::MasterDown
                        | redis::ErrorKind::ReadOnly
                        | redis::ErrorKind::IoError
                ) =>
            {
                self.conn.signal_needs_fix();
                true
            }
            _ => true,
        }
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        if response.is_successful() {
            RetryAction::Successful
        } else {
            RetryAction::Retry("Sending data to redis failed.".into())
        }
    }
}
