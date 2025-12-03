use std::{num::NonZeroUsize, time::Duration};

use redis::{AsyncTypedCommands, RedisError};
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use vector_lib::{
    config::{LogNamespace, clone_input_definitions},
    configurable::configurable_component,
    lookup::lookup_v2::OptionalValuePath,
};

use crate::{
    config::{
        DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    schema,
    template::Template,
};

use super::transform::RedisTransform;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to build redis client: {}", source))]
    Client { source: redis::RedisError },
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: RedisError },
}

/// Configuration for the `redis` transform.
#[serde_as]
#[configurable_component(transform("redis", "Enrich events with data from Redis lookups."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RedisTransformConfig {
    /// The Redis URL to connect to.
    ///
    /// The URL must take the form of `protocol://server:port/db` where the `protocol` can either be `redis` or `rediss` for connections secured using TLS.
    #[configurable(metadata(docs::examples = "redis://127.0.0.1:6379/0"))]
    pub url: String,

    /// The Redis key template to use for lookups.
    ///
    /// This template is evaluated for each event to determine the Redis key to look up.
    /// The template can use event fields using the `{{ field_name }}` syntax, for example: `user:{{ user_id }}`.
    #[configurable(metadata(
        docs::examples = "user:{{ user_id }}",
        docs::examples = "session:{{ session_id }}"
    ))]
    pub key: Template,

    /// The field path where the Redis lookup value will be stored.
    ///
    /// If the Redis key is not found, the field will not be added to the event.
    #[configurable(metadata(
        docs::examples = "redis_data",
        docs::examples = "enrichment.user_data"
    ))]
    pub output_field: OptionalValuePath,

    /// The default value to use if the Redis key is not found.
    ///
    /// If not specified, the field will not be added when the key is missing.
    #[configurable(metadata(docs::examples = "default_value", docs::examples = ""))]
    pub default_value: Option<String>,

    /// Maximum number of Redis lookup results to cache in memory.
    ///
    /// When set, Redis lookup results are cached to reduce round-trips to Redis.
    /// The cache uses an LRU (Least Recently Used) eviction policy.
    ///
    /// If not specified, caching is disabled and every lookup will query Redis.
    #[configurable(metadata(docs::examples = 1000, docs::examples = 10000))]
    pub cache_max_size: Option<NonZeroUsize>,

    /// Time-to-live (TTL) for cached Redis lookup results.
    ///
    /// When set, cached entries will expire after the specified duration.
    /// Expired entries are automatically refreshed from Redis on the next lookup
    /// instead of being evicted from the cache.
    ///
    /// If not specified, cached entries do not expire.
    #[serde_as(as = "Option<serde_with::DurationMilliSeconds<u64>>")]
    #[configurable(metadata(docs::examples = 300000, docs::examples = 3600000))]
    #[configurable(metadata(docs::type_unit = "milliseconds"))]
    pub cache_ttl: Option<Duration>,
}

impl GenerateConfig for RedisTransformConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            url = "redis://127.0.0.1:6379/0"
            key = "user:{{ user_id }}"
            output_field = "redis_data"
            "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "redis")]
impl TransformConfig for RedisTransformConfig {
    async fn build(
        &self,
        _context: &TransformContext,
    ) -> crate::Result<crate::transforms::Transform> {
        let redis_client = redis::Client::open(self.url.as_str()).context(ClientSnafu {})?;

        let mut conn = redis_client
            .get_connection_manager()
            .await
            .context(ConnectionSnafu {})?;

        // Test the connection with a ping
        conn.ping().await.context(ConnectionSnafu {})?;

        Ok(crate::transforms::Transform::event_task(
            RedisTransform::new(
                conn,
                self.key.clone(),
                self.output_field.clone(),
                self.default_value.clone(),
                self.cache_max_size,
                self.cache_ttl,
            ),
        ))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(
        &self,
        _enrichment_tables: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(
            DataType::all_bits(),
            clone_input_definitions(input_definitions),
        )]
    }

    fn enable_concurrency(&self) -> bool {
        true
    }
}
