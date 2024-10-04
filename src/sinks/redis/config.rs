use redis::{aio::ConnectionManager, RedisResult};
use snafu::prelude::*;

use crate::sinks::{prelude::*, util::service::TowerRequestConfigDefaults};

use super::{sink::RedisSink, RedisCreateFailedSnafu};

#[derive(Clone, Copy, Debug)]
pub struct RedisTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for RedisTowerRequestConfigDefaults {
    const CONCURRENCY: Concurrency = Concurrency::None;
}

/// Redis data type to store messages in.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum DataTypeConfig {
    /// The Redis `list` type.
    ///
    /// This resembles a deque, where messages can be popped and pushed from either end.
    ///
    /// This is the default.
    #[derivative(Default)]
    List,

    /// The Redis `channel` type.
    ///
    /// Redis channels function in a pub/sub fashion, allowing many-to-many broadcasting and receiving.
    Channel,
}

/// List-specific options.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub struct ListOption {
    /// The method to use for pushing messages into a `list`.
    pub(super) method: Method,
}

/// Method for pushing messages into a `list`.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    /// Use the `rpush` method.
    ///
    /// This pushes messages onto the tail of the list.
    ///
    /// This is the default.
    #[derivative(Default)]
    RPush,

    /// Use the `lpush` method.
    ///
    /// This pushes messages onto the head of the list.
    LPush,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RedisDefaultBatchSettings;

impl SinkBatchSettings for RedisDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `redis` sink.
#[configurable_component(sink("redis", "Publish observability data to Redis."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RedisSinkConfig {
    #[configurable(derived)]
    pub(super) encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) data_type: DataTypeConfig,

    #[configurable(derived)]
    #[serde(alias = "list")]
    pub(super) list_option: Option<ListOption>,

    /// The URL of the Redis endpoint to connect to.
    ///
    /// The URL _must_ take the form of `protocol://server:port/db` where the protocol can either be
    /// `redis` or `rediss` for connections secured via TLS.
    #[configurable(metadata(docs::examples = "redis://127.0.0.1:6379/0"))]
    #[serde(alias = "url")]
    pub(super) endpoint: String,

    /// The Redis key to publish messages to.
    #[configurable(validation(length(min = 1)))]
    #[configurable(metadata(docs::examples = "syslog:{{ app }}", docs::examples = "vector"))]
    pub(super) key: Template,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) batch: BatchConfig<RedisDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) request: TowerRequestConfig<RedisTowerRequestConfigDefaults>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(super) acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for RedisSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            url = "redis://127.0.0.1:6379/0"
            key = "vector"
            data_type = "list"
            list.method = "lpush"
            encoding.codec = "json"
            batch.max_events = 1
            "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "redis")]
impl SinkConfig for RedisSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        if self.key.is_empty() {
            return Err("`key` cannot be empty.".into());
        }
        let conn = self.build_client().await.context(RedisCreateFailedSnafu)?;
        let healthcheck = RedisSinkConfig::healthcheck(conn.clone()).boxed();
        let sink = RedisSink::new(self, conn)?;
        Ok((super::VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl RedisSinkConfig {
    pub(super) async fn build_client(&self) -> RedisResult<ConnectionManager> {
        let client = redis::Client::open(self.endpoint.as_str())?;
        client.get_connection_manager().await
    }

    async fn healthcheck(mut conn: ConnectionManager) -> crate::Result<()> {
        redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(Into::into)
    }
}
