use redis::{
    sentinel::{Sentinel, SentinelNodeConnectionInfo},
    ProtocolVersion, RedisConnectionInfo, TlsMode,
};
use snafu::prelude::*;

use crate::{
    serde::OneOrMany,
    sinks::{prelude::*, util::service::TowerRequestConfigDefaults},
};

use super::{
    sink::{RedisConnection, RedisSink},
    RedisCreateFailedSnafu,
};

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
    pub(super) endpoint: OneOrMany<String>,

    /// The service name to use for sentinel.
    ///
    /// If this is specified, `endpoint` will be used to reach sentinel instances instead of a
    /// redis instance.
    #[configurable]
    pub(super) sentinel_service: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) sentinel_connect: Option<SentinelConnectionSettings>,

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
        let conn = self.build_connection().await?;
        let healthcheck = RedisSinkConfig::healthcheck(conn.clone()).boxed();
        let sink = RedisSink::new(self, conn)?;
        Ok((super::VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl RedisSinkConfig {
    pub(super) async fn build_connection(&self) -> crate::Result<RedisConnection> {
        let endpoints = self.endpoint.clone().to_vec();

        if endpoints.is_empty() {
            return Err("`endpoint` cannot be empty.".into());
        }

        if let Some(sentinel_service) = &self.sentinel_service {
            let sentinel = Sentinel::build(endpoints).context(RedisCreateFailedSnafu)?;

            Ok(RedisConnection::new_sentinel(
                sentinel,
                sentinel_service.clone(),
                self.sentinel_connect.clone().unwrap_or_default().into(),
            )
            .await
            .context(RedisCreateFailedSnafu)?)
        } else {
            // SAFETY: endpoints cannot be empty (checked above)
            let client =
                redis::Client::open(endpoints[0].as_str()).context(RedisCreateFailedSnafu)?;
            let conn = client
                .get_connection_manager()
                .await
                .context(RedisCreateFailedSnafu)?;

            Ok(RedisConnection::new_direct(conn))
        }
    }

    async fn healthcheck(mut conn: RedisConnection) -> crate::Result<()> {
        redis::cmd("PING")
            .query_async(&mut conn.get_connection_manager().await?.connection)
            .await
            .map_err(Into::into)
    }
}

/// Controls how Redis Sentinel will connect to the servers belonging to it.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct SentinelConnectionSettings {
    #[configurable(derived)]
    #[serde(default)]
    pub tls: MaybeTlsMode,

    #[configurable(derived)]
    #[serde(default)]
    pub connections: Option<RedisConnectionSettings>,
}

impl From<SentinelConnectionSettings> for SentinelNodeConnectionInfo {
    fn from(value: SentinelConnectionSettings) -> Self {
        SentinelNodeConnectionInfo {
            tls_mode: value.tls.into(),
            redis_connection_info: value.connections.map(Into::into),
        }
    }
}

/// How/if TLS should be established.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum MaybeTlsMode {
    /// Don't use TLS.
    ///
    /// This is the default.
    #[derivative(Default)]
    None,

    /// Enable TLS with certificate verification.
    Secure,

    /// Enable TLS without certificate verification.
    Insecure,
}

impl From<MaybeTlsMode> for Option<TlsMode> {
    fn from(value: MaybeTlsMode) -> Self {
        match value {
            MaybeTlsMode::None => None,
            MaybeTlsMode::Secure => Some(TlsMode::Secure),
            MaybeTlsMode::Insecure => Some(TlsMode::Insecure),
        }
    }
}

/// Connection independent information used to establish a connection
/// to a redis instance sentinel owns.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub struct RedisConnectionSettings {
    /// The database number to use. Usually `0`.
    pub db: i64,

    /// Optionally, the username to connection with.
    pub username: Option<String>,

    /// Optionally, the password to connection with.
    pub password: Option<String>,

    /// The version of RESP to use.
    pub protocol: RedisProtocolVersion,
}

impl From<RedisConnectionSettings> for RedisConnectionInfo {
    fn from(value: RedisConnectionSettings) -> Self {
        RedisConnectionInfo {
            db: value.db,
            username: value.username,
            password: value.password,
            protocol: value.protocol.into(),
        }
    }
}

/// The communication protocol to use with the redis server.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
pub enum RedisProtocolVersion {
    /// Use RESP2.
    ///
    /// This is the default.
    #[derivative(Default)]
    RESP2,

    /// Use RESP3.
    RESP3,
}

impl From<RedisProtocolVersion> for ProtocolVersion {
    fn from(value: RedisProtocolVersion) -> Self {
        match value {
            RedisProtocolVersion::RESP2 => ProtocolVersion::RESP2,
            RedisProtocolVersion::RESP3 => ProtocolVersion::RESP3,
        }
    }
}
