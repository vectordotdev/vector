use futures::FutureExt;
use tower::ServiceBuilder;
use ydb::ClientBuilder;
use vector_lib::{
    config::AcknowledgementsConfig,
    configurable::{component::GenerateConfig, configurable_component},
    sink::VectorSink,
};

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
    #[configurable(metadata(docs::examples = "grpc://localhost:2136?database=/local"))]
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
        let client = ClientBuilder::new_from_connection_string(&self.endpoint)?
            .client()?;

        client.wait().await?;

        let table_client = client.table_client();

        let healthcheck = healthcheck(table_client.clone()).boxed();

        let service = YdbService::new(
            table_client,
            self.table.clone(),
            self.endpoint.clone(),
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
        Input::all()
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
