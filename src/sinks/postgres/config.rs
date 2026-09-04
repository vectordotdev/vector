use std::str::FromStr;

use futures::FutureExt;
use sqlx::{
    Pool, Postgres,
    postgres::{PgConnectOptions, PgPoolOptions},
};
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
            TowerRequestConfig, TowerRequestSettings, UriSerde, uri::protocol_endpoint,
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
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for PostgresConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"endpoint: "postgres://user:password@localhost/default"
            table: table
        "#,
        })
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

#[derive(Clone, Debug)]
pub struct ValidatedPostgres {
    batch_settings: BatcherSettings,
    request_settings: TowerRequestSettings,
    endpoint_uri: UriSerde,
}

/// PostgreSQL endpoints may carry credentials as userinfo or as a `password`
/// query parameter (SQLx percent-decodes query keys), so they are always
/// redacted from error messages.
fn redact_endpoint(_endpoint: &str) -> String {
    "<redacted endpoint>".to_owned()
}

/// Validates the PostgreSQL connection string without touching the network or
/// filesystem.
///
/// SQLx applies `.pgpass` when a connection string has no password. An empty
/// password is appended to the validation-only URL so SQLx validates every
/// option without reading `$PGPASSFILE` or `~/.pgpass`.
fn validate_pg_endpoint(endpoint: &str) -> crate::Result<()> {
    let mut url = url::Url::parse(endpoint).map_err(|e| {
        format!(
            "invalid PostgreSQL connection string `{}`: {e}",
            redact_endpoint(endpoint)
        )
    })?;
    if !matches!(url.scheme(), "postgres" | "postgresql") {
        return Err(format!(
            "invalid PostgreSQL connection string `{}`: expected a \
             `postgres://` or `postgresql://` URL, got scheme `{}`",
            redact_endpoint(endpoint),
            url.scheme()
        )
        .into());
    }
    url.query_pairs_mut().append_pair("password", "");
    pg_connect_options(url.as_str()).map(|_| ())
}

/// Parses the PostgreSQL connection string into SQLx connect options.
///
/// Unlike [`validate_pg_endpoint`], this may read `$PGPASSFILE` or `~/.pgpass`
/// to resolve a missing password, so it is only invoked at build time.
fn pg_connect_options(endpoint: &str) -> crate::Result<PgConnectOptions> {
    PgConnectOptions::from_str(endpoint).map_err(|e| {
        format!(
            "invalid PostgreSQL connection string `{}`: {e}",
            redact_endpoint(endpoint)
        )
        .into()
    })
}

#[async_trait::async_trait]
impl ValidatedSink for PostgresConfig {
    type Validated = ValidatedPostgres;

    fn validate(&self) -> crate::Result<ValidatedPostgres> {
        if self.pool_size == 0 {
            return Err("`pool_size` must be greater than zero".into());
        }

        let batch_settings = self.batch.into_batcher_settings()?;
        let request_settings = self.request.into_settings();
        let endpoint_uri: UriSerde = self.endpoint.parse()?;
        validate_pg_endpoint(&self.endpoint)?;

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
        } = validated.clone();

        // `PgConnectOptions::from_str` may read `$PGPASSFILE` or `~/.pgpass`
        // to resolve a missing password, so it runs here rather than in
        // `validate`.
        let pg_connect_options = pg_connect_options(&self.endpoint)?;

        let connection_pool = PgPoolOptions::new()
            .max_connections(self.pool_size)
            .connect_lazy_with(pg_connect_options);

        let healthcheck = healthcheck(connection_pool.clone()).boxed();

        // The endpoint label must not carry credentials or query parameters.
        let endpoint = protocol_endpoint(endpoint_uri.uri.clone()).1;
        let service = PostgresService::new(connection_pool, self.table.clone(), endpoint);
        let service = ServiceBuilder::new()
            .settings(request_settings, PostgresRetryLogic)
            .service(service);

        let sink = PostgresSink::new(service, batch_settings);

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

    #[test]
    fn validate_accepts_valid_postgres_dsn() {
        use crate::config::ValidatedSink;

        let cfg = serde_yaml::from_str::<PostgresConfig>(indoc::indoc! {r#"
            endpoint: "postgres://user:password@localhost/default"
            table: "mytable"
        "#})
        .unwrap();
        cfg.validate().expect("valid postgres DSN should validate");
    }

    #[test]
    fn validate_rejects_zero_pool_size() {
        use crate::config::ValidatedSink;

        let cfg = serde_yaml::from_str::<PostgresConfig>(indoc::indoc! {r#"
            endpoint: "postgres://user:password@localhost/default"
            table: "mytable"
            pool_size: 0
        "#})
        .unwrap();
        assert!(
            cfg.validate().is_err(),
            "zero connection pool size should not validate"
        );
    }

    #[test]
    fn validate_rejects_invalid_sqlx_dsn_option() {
        use crate::config::ValidatedSink;

        let cfg = serde_yaml::from_str::<PostgresConfig>(indoc::indoc! {r#"
            endpoint: "postgres://user:password@localhost/default?sslmode=bogus"
            table: "mytable"
        "#})
        .unwrap();
        assert!(
            cfg.validate().is_err(),
            "invalid SQLx DSN option should not validate"
        );
    }

    #[test]
    fn validate_rejects_non_postgres_uri() {
        use crate::config::ValidatedSink;

        let cfg = serde_yaml::from_str::<PostgresConfig>(indoc::indoc! {r#"
            endpoint: "https://example.com"
            table: "mytable"
        "#})
        .unwrap();
        let err = cfg
            .validate()
            .expect_err("generic URI should not validate as a postgres DSN");
        assert!(
            err.to_string()
                .contains("invalid PostgreSQL connection string"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_malformed_postgres_dsn() {
        use crate::config::ValidatedSink;

        let cfg = serde_yaml::from_str::<PostgresConfig>(indoc::indoc! {r#"
            endpoint: "postgres://user:password@localhost:notaport/default"
            table: "mytable"
        "#})
        .unwrap();
        assert!(
            cfg.validate().is_err(),
            "malformed postgres DSN should not validate"
        );
    }

    #[test]
    fn validate_redacts_credentials_from_errors() {
        use crate::config::ValidatedSink;

        // A valid URI with credentials but a non-`postgres` scheme parses as a
        // `UriSerde` and reaches `parse_pg_connect_options`, whose error must
        // not echo the credentials back into validation output.
        let cfg = serde_yaml::from_str::<PostgresConfig>(indoc::indoc! {r#"
            endpoint: "https://user:secret@example.com/db"
            table: "mytable"
        "#})
        .unwrap();
        let err = cfg
            .validate()
            .expect_err("non-postgres URI should not validate as a postgres DSN");
        let message = err.to_string();
        assert!(
            !message.contains("secret"),
            "credentials must not leak into validation errors: {message}"
        );
        assert!(
            message.contains("<redacted endpoint>"),
            "unexpected error: {message}"
        );
    }

    #[test]
    fn validate_redacts_password_query_param_from_errors() {
        use crate::config::ValidatedSink;

        // SQLx also accepts the password as a `password` query parameter; a
        // DSN carrying one must not echo it into validation errors.
        let cfg = serde_yaml::from_str::<PostgresConfig>(indoc::indoc! {r#"
            endpoint: "https://example.com/db?password=secret"
            table: "mytable"
        "#})
        .unwrap();
        let err = cfg
            .validate()
            .expect_err("non-postgres URI should not validate as a postgres DSN");
        let message = err.to_string();
        assert!(
            !message.contains("secret"),
            "credentials must not leak into validation errors: {message}"
        );
        assert!(
            message.contains("<redacted endpoint>"),
            "unexpected error: {message}"
        );
    }
}
