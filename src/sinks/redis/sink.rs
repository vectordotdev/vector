use std::{
    future,
    sync::{Arc, Mutex as StdMutex},
};

use redis::{
    aio::ConnectionManager,
    sentinel::{Sentinel, SentinelNodeConnectionInfo},
    RedisResult,
};
use tokio::sync::Mutex as TokioMutex;

use crate::sinks::{prelude::*, redis::RedisSinkError, util::retries::RetryAction};

use super::{
    config::{DataTypeConfig, RedisSinkConfig, RedisTowerRequestConfigDefaults},
    request_builder::request_builder,
    service::{RedisResponse, RedisService},
    RedisEvent,
};

pub(super) type GenerationCount = u64;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RepairState {
    Repaired,
    UnknownBroken,
    Broken(GenerationCount),
}

#[derive(Clone)]
pub(super) struct ConnectionStateInner {
    connection: ConnectionManager,
    generation: GenerationCount,
}

impl From<ConnectionStateInner> for ConnectionState {
    fn from(value: ConnectionStateInner) -> Self {
        ConnectionState {
            connection: value.connection,
            generation: Some(value.generation),
        }
    }
}

#[derive(Clone)]
pub(super) struct ConnectionState {
    pub connection: ConnectionManager,
    pub generation: Option<GenerationCount>,
}

impl ConnectionState {
    pub fn new_no_generation(conn: ConnectionManager) -> Self {
        Self {
            connection: conn,
            generation: None,
        }
    }
}

#[derive(Clone)]
pub(super) enum RedisConnection {
    Direct(ConnectionManager),
    Sentinel {
        // Tokio's Mutex was used instead of std since we need to hold it
        // across await points in [`Self::get_connection_manager`]
        sentinel: Arc<TokioMutex<Sentinel>>,
        service_name: String,
        node_connection_info: SentinelNodeConnectionInfo,
        // Track the `ConnectionManager` and an id to associate with this
        // `ConnectionManager` for use during error handling
        connection_state: Arc<TokioMutex<ConnectionStateInner>>,
        // State to track how the `connection_manager` needs to be repaired as
        // we cannot call async methods to reapir the redis connection from
        // sentinel with the sync `RetryLogic::on_retriable_error`.
        repair_state: Arc<StdMutex<RepairState>>,
    },
}

impl RedisConnection {
    pub(super) const fn new_direct(conn: ConnectionManager) -> Self {
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
            sentinel: Arc::new(TokioMutex::new(sentinel)),
            service_name,
            node_connection_info,
            connection_state: Arc::new(TokioMutex::new(ConnectionStateInner {
                connection: conn,
                generation: 0,
            })),
            repair_state: Arc::new(StdMutex::new(RepairState::Repaired)),
        })
    }

    pub(super) async fn get_connection_manager(&self) -> RedisResult<ConnectionState> {
        match self {
            Self::Direct(conn) => Ok(ConnectionState::new_no_generation(conn.clone())),
            Self::Sentinel {
                sentinel,
                service_name,
                node_connection_info,
                connection_state,
                repair_state,
            } => {
                let mut conn_state = connection_state.lock().await;

                // Scope needed since Rust borrow checker cannot understand the explicitly dropped
                // MutexGuard isn't held anymore.
                // See: https://github.com/rust-lang/rust/issues/128095
                let connection_needs_repair = {
                    let mut repair_state = repair_state.lock().expect("poisoned lock");

                    match *repair_state {
                        RepairState::Repaired => false,
                        RepairState::Broken(id) if id != conn_state.generation => {
                            // Disregard since we're on a different connection manager now
                            *repair_state = RepairState::Repaired;
                            false
                        }
                        _ => true,
                    }
                };

                if !connection_needs_repair {
                    return Ok(conn_state.clone().into());
                }

                let mut sentinel = sentinel.lock().await;

                conn_state.connection = Self::sentinel_connection_manager(
                    &mut sentinel,
                    service_name.as_str(),
                    node_connection_info,
                )
                .await?;
                conn_state.generation = conn_state.generation.wrapping_add(1);

                // Have to reacquire since we needed to do a few awaits, we can safely override
                // it as we are the only thread that could've mutated the connection manager.
                let mut repair_state = repair_state.lock().expect("poisoned lock");
                *repair_state = RepairState::Repaired;

                Ok(conn_state.clone().into())
            }
        }
    }

    pub(super) fn signal_broken(&self, generation: Option<GenerationCount>) {
        if let Self::Sentinel { repair_state, .. } = self {
            let mut state = repair_state.lock().expect("poisoned lock");

            match (*state, generation) {
                (RepairState::Broken(_) | RepairState::Repaired, None) => {
                    *state = RepairState::UnknownBroken
                }
                (RepairState::Broken(_) | RepairState::Repaired, Some(id)) => {
                    *state = RepairState::Broken(id)
                }
                (RepairState::UnknownBroken, _) => (),
            }
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
                    connection: self.conn.clone(),
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
    connection: RedisConnection,
}

impl RetryLogic for RedisRetryLogic {
    type Error = RedisSinkError;
    type Response = RedisResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn on_retriable_error(&self, error: &Self::Error) {
        if let RedisSinkError::SendError { source, generation } = error {
            if matches!(
                source.kind(),
                redis::ErrorKind::MasterDown
                    | redis::ErrorKind::ReadOnly
                    | redis::ErrorKind::IoError
            ) {
                self.connection.signal_broken(generation.clone());
            }
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
