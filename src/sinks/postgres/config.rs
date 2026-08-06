use futures::FutureExt;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use tower::ServiceBuilder;
use vector_lib::{
    config::AcknowledgementsConfig,
    configurable::{component::GenerateConfig, configurable_component},
    sink::VectorSink,
    stream::BatcherSettings,
};

use super::{
    service::{PostgresRetryLogic, PostgresService},
    sink::PostgresSink,
};
use crate::{
    config::{Input, SinkConfig, SinkContext, ValidatedSink},
    sinks::{
        Healthcheck,
        util::{
            BatchConfig, RealtimeSizeBasedDefaultBatchSettings, ServiceBuilderExt,
            TowerRequestConfig, TowerRequestSettings, UriSerde,
        },
    },
};

const fn default_pool_size() -> u32 {
    5
}

/// Configuration for the `postgres` sink.
#[configurable_component(sink("postgres", "Deliver log data to a PostgreSQL database."))]
#[derive(Clone, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct PostgresConfig {
    /// The PostgreSQL server connection string. It can contain the username and password.
    /// See [PostgreSQL documentation](https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING) about connection strings for more information
    /// about valid formats and options that can be used.
    pub endpoint: String,

    /// The table that data is inserted into. This table parameter is vulnerable
    /// to SQL injection attacks as Vector does not validate or sanitize it, you must not use untrusted input.
    /// This parameter will be directly interpolated in the SQL query statement,
    /// as table names as parameters in prepared statements are not allowed in PostgreSQL.
    pub table: String,

    /// The postgres connection pool size. See [this](https://docs.rs/sqlx/latest/sqlx/struct.Pool.html#why-use-a-pool) for more
    /// information about why a connection pool should be used.
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    /// Event batching behavior.
    ///
    /// Note that as PostgreSQL's `jsonb_populate_recordset` function is used to insert events,
    /// a single event in the batch can make the whole batch to fail. For example, if a single event within the batch triggers
    /// a unique constraint violation in the destination table, the whole event batch will fail.
    ///
    /// As a workaround, [triggers](https://www.postgresql.org/docs/current/sql-createtrigger.html) on constraint violations
    /// can be defined at a database level to change the behavior of the insert operation on specific tables.
    /// Alternatively, setting `max_events` batch setting to `1` will make each event to be inserted independently,
    /// so events that trigger a constraint violation will not affect the rest of the events.
    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for PostgresConfig {
    fn generate_config() -> serde_json::Value {
        toml::from_str(
            r#"endpoint = "postgres://user:password@localhost/default"
            table = "table"
        "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "postgres")]
impl SinkConfig for PostgresConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Purely validated PostgreSQL sink configuration.
///
/// This type captures all validation results that can be computed purely from
/// configuration without network/filesystem/credentials/async operations.
/// The actual sink building consumes these values without recomputing them.
#[derive(Clone, Debug)]
pub struct ValidatedPostgres {
    /// Batch settings computed during validation.
    batch_settings: BatcherSettings,
    /// Request settings computed during validation.
    request_settings: TowerRequestSettings,
    /// The parsed endpoint URI, validated.
    endpoint_uri: UriSerde,
}

#[async_trait::async_trait]
impl ValidatedSink for PostgresConfig {
    type Validated = ValidatedPostgres;

    fn validate(&self) -> crate::Result<ValidatedPostgres> {
        let batch_settings = self.batch.into_batcher_settings()?;
        let request_settings = self.request.into_settings();
        let endpoint_uri: UriSerde = self.endpoint.parse()?;

        Ok(ValidatedPostgres {
            batch_settings,
            request_settings,
            endpoint_uri,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedPostgres,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedPostgres {
            batch_settings,
            request_settings,
            endpoint_uri,
        } = validated;

        let connection_pool = PgPoolOptions::new()
            .max_connections(self.pool_size)
            .connect_lazy(&self.endpoint)?;

        let healthcheck = healthcheck(connection_pool.clone()).boxed();

        let service = PostgresService::new(
            connection_pool,
            self.table.clone(),
            endpoint_uri.to_string(),
        );
        let service = ServiceBuilder::new()
            .settings(request_settings.clone(), PostgresRetryLogic)
            .service(service);

        let sink = PostgresSink::new(service, *batch_settings);

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

async fn healthcheck(connection_pool: Pool<Postgres>) -> crate::Result<()> {
    sqlx::query("SELECT 1").execute(&connection_pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PostgresConfig>();
    }

    #[test]
    fn parse_config() {
        let cfg = serde_yaml::from_str::<PostgresConfig>(indoc::indoc! {r#"
            endpoint: "postgres://user:password@localhost/default"
            table: "mytable"
        "#})
        .unwrap();
        assert_eq!(cfg.endpoint, "postgres://user:password@localhost/default");
        assert_eq!(cfg.table, "mytable");
    }

    #[test]
    fn validate_produces_usable_values() {
        use crate::config::ValidatedSink;

        let cfg = serde_yaml::from_str::<PostgresConfig>(indoc::indoc! {r#"
            endpoint: "postgres://user:password@localhost/default"
            table: "mytable"
        "#})
        .unwrap();
        let validated = cfg.validate().expect("validation should succeed");
        // The parsed endpoint URI redacts the password in its Display output.
        assert_eq!(validated.endpoint_uri.uri.host(), Some("localhost"));
        assert_eq!(validated.endpoint_uri.uri.path(), "/default");
    }
}
