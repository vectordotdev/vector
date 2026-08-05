use std::sync::{Arc, Mutex};

use futures::FutureExt;
use tower::ServiceBuilder;
use vector_lib::{
    config::AcknowledgementsConfig,
    configurable::{component::GenerateConfig, configurable_component},
    sink::VectorSink,
};

use super::{
    schema::fetch_table_schema,
    service::{
        DuckdbRetryLogic, DuckdbService, build_serializer, default_database, open_connection,
    },
    sink::DuckdbSink,
};
use crate::{
    config::{Input, SinkConfig, SinkContext},
    sinks::{
        Healthcheck,
        util::{
            BatchConfig, RealtimeSizeBasedDefaultBatchSettings, ServiceBuilderExt,
            TowerRequestConfig,
        },
    },
};

/// Configuration for the `duckdb` sink.
#[configurable_component(sink("duckdb", "Deliver log data to a DuckDB database."))]
#[derive(Clone, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct DuckdbConfig {
    /// The DuckDB database endpoint.
    ///
    /// Use a filesystem path or a `duckdb://` URI such as
    /// `duckdb:///var/lib/vector/events.duckdb`.
    #[configurable(metadata(docs::examples = "duckdb:///var/lib/vector/events.duckdb"))]
    pub endpoint: String,

    /// The table that data is inserted into.
    ///
    /// The table must already exist. Vector reads its schema at startup and encodes each batch to
    /// match the destination table before appending it.
    #[configurable(metadata(docs::examples = "events"))]
    pub table: String,

    /// The DuckDB database/schema containing the table.
    #[serde(default = "default_database")]
    #[configurable(metadata(docs::examples = "main"))]
    pub database: String,

    /// Event batching behavior.
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

impl GenerateConfig for DuckdbConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "duckdb:///var/lib/vector/events.duckdb"
            table = "events"
        "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "duckdb")]
impl SinkConfig for DuckdbConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = self.endpoint.clone();
        let table = self.table.clone();
        let database = self.database.clone();

        let connection = tokio::task::spawn_blocking({
            let endpoint = endpoint.clone();
            move || open_connection(&endpoint)
        })
        .await??;

        let schema = fetch_table_schema(&connection, &database, &table)?;
        let serializer = build_serializer(schema)?;

        let connection = Arc::new(Mutex::new(connection));

        let healthcheck =
            healthcheck(Arc::clone(&connection), database.clone(), table.clone()).boxed();

        let batch_settings = self.batch.into_batcher_settings()?;
        let request_settings = self.request.into_settings();

        let service = DuckdbService::new(connection, database, table, endpoint, serializer);
        let service = ServiceBuilder::new()
            .settings(request_settings, DuckdbRetryLogic)
            .service(service);

        let sink = DuckdbSink::new(service, batch_settings);

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

async fn healthcheck(
    connection: Arc<Mutex<duckdb::Connection>>,
    database: String,
    table: String,
) -> crate::Result<()> {
    tokio::task::spawn_blocking(move || -> crate::Result<()> {
        let conn = connection
            .lock()
            .map_err(|_| -> crate::Error { "Connection mutex poisoned".into() })?;
        conn.execute("SELECT 1", [])?;
        fetch_table_schema(&conn, &database, &table)?;
        Ok(())
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DuckdbConfig>();
    }

    #[test]
    fn parse_config() {
        let cfg = serde_yaml::from_str::<DuckdbConfig>(indoc::indoc! {r#"
            endpoint: "duckdb:///tmp/vector.duckdb"
            table: "events"
        "#})
        .unwrap();
        assert_eq!(cfg.endpoint, "duckdb:///tmp/vector.duckdb");
        assert_eq!(cfg.table, "events");
        assert_eq!(cfg.database, "main");
    }
}
