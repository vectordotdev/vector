use futures::FutureExt;
use tower::ServiceBuilder;
use tracing::debug;
use vector_lib::{
    config::{AcknowledgementsConfig, DataType},
    configurable::{component::GenerateConfig, configurable_component},
    sink::VectorSink,
};
use ydb::{ClientBuilder, IndexType, TableDescription};

use super::{
    service::{YdbRetryLogic, YdbService},
    sink::YdbSink,
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

/// Configuration for the `ydb` sink.
#[configurable_component(sink("ydb", "Deliver log data to a YDB (Yandex Database)."))]
#[derive(Clone, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct YdbConfig {
    /// The YDB connection string (gRPC endpoint with database).
    ///
    #[configurable(metadata(
        docs::examples = "grpc://localhost:2136?database=/local",
        docs::examples = "grpcs://ydb.example.com:2135?database=/local&ca_certificate=/path/to/ca.pem"
    ))]
    pub endpoint: String,

    /// The YDB table path to insert data into.
    ///
    /// Must be a full absolute path from the database root, starting with `/`.
    /// The table must already exist with the required schema.
    #[configurable(metadata(docs::examples = "/local/logs"))]
    pub table: String,

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InsertStrategy {
    BulkUpsert,
    Upsert,
}

pub(crate) fn choose_insert_strategy(schema: &TableDescription) -> InsertStrategy {
    let has_sync_indexes = schema
        .indexes
        .iter()
        .any(|idx| matches!(idx.index_type, IndexType::Global | IndexType::GlobalUnique));

    if has_sync_indexes {
        InsertStrategy::Upsert
    } else {
        InsertStrategy::BulkUpsert
    }
}

impl GenerateConfig for YdbConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "grpc://localhost:2136?database=/local"
            table = "/local/logs"
        "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "ydb")]
impl SinkConfig for YdbConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = ClientBuilder::new_from_connection_string(&self.endpoint)?.client()?;

        client.wait().await?;

        let table_client = client.table_client();

        let healthcheck = healthcheck(table_client.clone()).boxed();

        let table_schema = table_client
            .describe_table(self.table.clone())
            .await
            .map_err(|e| format!("Failed to fetch table schema for '{}': {}", self.table, e))?;

        debug!(
            message = "Fetched YDB table schema",
            table = %self.table,
            columns = table_schema.columns.len(),
            primary_key = ?table_schema.primary_key,
        );

        let service = YdbService::new(
            table_client,
            self.table.clone(),
            self.endpoint.clone(),
            table_schema,
        );

        let batch_settings = self.batch.into_batcher_settings()?;
        let request_settings = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_settings, YdbRetryLogic)
            .service(service);

        let sink = YdbSink::new(service, batch_settings);

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log | DataType::Trace)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

async fn healthcheck(table_client: ydb::TableClient) -> crate::Result<()> {
    let table_client = table_client.clone_with_transaction_options(
        ydb::TransactionOptions::new()
            .with_mode(ydb::Mode::OnlineReadonly)
            .with_autocommit(true),
    );

    table_client
        .retry_transaction(|mut t| async move {
            t.query(ydb::Query::new("SELECT 1")).await?;
            Ok(())
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<YdbConfig>();
    }

    #[test]
    fn parse_config() {
        let cfg = toml::from_str::<YdbConfig>(
            r#"
            endpoint = "grpc://localhost:2136?database=/local"
            table = "/local/logs"
        "#,
        )
        .unwrap();
        assert_eq!(cfg.endpoint, "grpc://localhost:2136?database=/local");
        assert_eq!(cfg.table, "/local/logs");
    }
}
