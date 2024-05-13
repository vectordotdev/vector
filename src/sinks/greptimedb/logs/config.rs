use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::greptimedb::healthcheck;
use crate::sinks::greptimedb::logs::sink;
use crate::sinks::{
    greptimedb::{
        default_dbname, GreptimeDBDefaultBatchSettings, GreptimeDBRetryLogic, GreptimeDBService,
    },
    prelude::*,
};

/// Configuration for the `greptimedb_logs` sink.
#[configurable_component(sink("greptimedb_logs", "Ingest logs data into GreptimeDB."))]
#[derive(Clone, Debug, Default, Derivative)]
#[serde(deny_unknown_fields)]
pub struct GreptimeDBLogsConfig {
    /// The endpoint of the GreptimeDB server.
    #[serde(alias = "host")]
    #[configurable(metadata(docs::examples = "localhost:4001"))]
    pub endpoint: String,

    /// The table that data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: String,

    /// The GreptimeDB [database][database] name to connect.
    ///
    /// Default to `public`, the default database of GreptimeDB.
    ///
    /// Database can be created via `create database` statement on
    /// GreptimeDB. If you are using GreptimeCloud, use `dbname` from the
    /// connection information of your instance.
    ///
    /// [database]: https://docs.greptime.com/user-guide/concepts/key-concepts#database
    #[configurable(metadata(docs::examples = "public"))]
    #[derivative(Default(value = "default_dbname()"))]
    #[serde(default = "default_dbname")]
    pub dbname: String,

    /// The username for your GreptimeDB instance.
    ///
    /// This is required if your instance has authentication enabled.
    #[configurable(metadata(docs::examples = "username"))]
    #[serde(default)]
    pub username: Option<String>,
    /// The password for your GreptimeDB instance.
    ///
    /// This is required if your instance has authentication enabled.
    #[configurable(metadata(docs::examples = "password"))]
    #[serde(default)]
    pub password: Option<SensitiveString>,

    #[configurable(derived)]
    #[serde(default)]
    pub(crate) batch: BatchConfig<GreptimeDBDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl_generate_config_from_default!(GreptimeDBLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "greptimedb_logs")]
impl SinkConfig for GreptimeDBLogsConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let request_settings = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_settings, GreptimeDBRetryLogic)
            .service(GreptimeDBService::try_new(self)?);
        let sink = sink::GreptimeDBLogsSink {
            _service: service,
            batch_settings: self.batch.into_batcher_settings()?,
            table: self.table.clone(),
        };

        let healthcheck = healthcheck(self)?;
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
