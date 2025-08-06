use snafu::prelude::*;
use std::{future, sync::Arc, time::Duration};

use redis::{
    aio::ConnectionManager,
    sentinel::{Sentinel, SentinelNodeConnectionInfo},
    RedisResult,
};
use tokio::{
    sync::watch::{self, Receiver, Sender},
    task::JoinHandle,
    time::sleep,
};

use crate::sinks::{prelude::*, redis::RedisSinkError, util::retries::RetryAction};

use super::{
    config::{DataTypeConfig, RedisSinkConfig, RedisTowerRequestConfigDefaults},
    request_builder::request_builder,
    service::{RedisResponse, RedisService},
    RedisEvent, RepairChannelSnafu,
};

pub(super) type GenerationCount = u64;

pub(super) enum RepairState {
    Active { state: ConnectionStateInner },
    Broken,
}

impl RepairState {
    pub(super) const fn needs_repair(&self) -> bool {
        matches!(self, RepairState::Broken)
    }

    pub(super) const fn is_active(&self) -> bool {
        matches!(self, RepairState::Active { .. })
    }

    pub(super) const fn get_active_state(&self) -> Option<&ConnectionStateInner> {
        if let RepairState::Active { state } = self {
            Some(state)
        } else {
            None
        }
    }
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
    pub const fn new_no_generation(conn: ConnectionManager) -> Self {
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
        connection_recv: Receiver<RepairState>,
        connection_send: Sender<RepairState>,
        // Background task that fixes the redis connection when it breaks
        repair_task: Arc<JoinHandle<()>>,
    },
}

impl Drop for RedisConnection {
    fn drop(&mut self) {
        // Stop repair task if all connections are dropped
        let Self::Sentinel { repair_task, .. } = self else {
            return;
        };
        let Some(repair_task) = Arc::get_mut(repair_task) else {
            return;
        };
        repair_task.abort();
    }
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

        let (conn_tx, conn_rx) = watch::channel(RepairState::Active {
            state: ConnectionStateInner {
                connection: conn,
                generation: 0,
            },
        });

        let task_conn_tx = conn_tx.clone();

        Ok(Self::Sentinel {
            connection_send: conn_tx,
            connection_recv: conn_rx,
            repair_task: Arc::new(tokio::spawn(async move {
                Self::repair_connection_manager_task(
                    sentinel,
                    service_name,
                    node_connection_info,
                    task_conn_tx,
                )
                .await
            })),
        })
    }

    async fn repair_connection_manager_task(
        mut sentinel: Sentinel,
        service_name: String,
        node_connection_info: SentinelNodeConnectionInfo,
        conn_send: Sender<RepairState>,
    ) -> ! {
        let mut conn_recv = conn_send.subscribe();
        let mut current_generation: GenerationCount = 0;
        let mut repairing = false;

        loop {
            if !repairing {
                // Wait until a repair is needed
                if let Err(error) = conn_recv.wait_for(|state| state.needs_repair()).await {
                    warn!("Connection state channel was dropped {error:?}.");
                    continue;
                }

                repairing = true;
            }

            let new_state = match Self::sentinel_connection_manager(
                &mut sentinel,
                service_name.as_str(),
                &node_connection_info,
            )
            .await
            {
                Ok(new_conn) => {
                    current_generation = current_generation.wrapping_add(1);

                    ConnectionStateInner {
                        connection: new_conn,
                        generation: current_generation,
                    }
                }
                Err(error) => {
                    warn!("Failed to repair ConnectionManager via sentinel (gen: {current_generation}): {error:?}.");
                    sleep(Duration::from_millis(250)).await;
                    continue;
                }
            };

            conn_send.send_modify(|state| {
                *state = RepairState::Active { state: new_state };
                repairing = false;
                debug!("Connection manager repaired successfully (new generation: {current_generation}).");
            });
        }
    }

    pub(super) async fn get_connection_manager(
        &mut self,
    ) -> Result<ConnectionState, RedisSinkError> {
        match self {
            Self::Direct(conn) => Ok(ConnectionState::new_no_generation(conn.clone())),
            Self::Sentinel {
                connection_recv, ..
            } => {
                match connection_recv.wait_for(|state| state.is_active()).await {
                    Ok(repair_state) => {
                        // SAFETY: we wait until we're in the active state before this runs
                        let state = repair_state
                            .get_active_state()
                            .expect("wait invariant broken");

                        Ok(state.clone().into())
                    }
                    Err(error) => Err(error).context(RepairChannelSnafu),
                }
            }
        }
    }

    pub(super) fn signal_broken(&self, generation: Option<GenerationCount>) {
        if let Self::Sentinel {
            connection_send, ..
        } = self
        {
            connection_send.send_if_modified(|state| {
                if let RepairState::Active { state: conn_state } = state {
                    match generation {
                        // If old generation is bad, disregard
                        Some(broken_gen) if broken_gen != conn_state.generation => false,
                        _ => {
                            *state = RepairState::Broken;
                            true
                        }
                    }
                } else {
                    // If already broken, disregard
                    false
                }
            });
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
                self.connection.signal_broken(*generation);
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
