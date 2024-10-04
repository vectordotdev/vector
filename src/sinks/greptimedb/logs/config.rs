use crate::{
    http::{Auth, HttpClient},
    sinks::{
        greptimedb::{
            default_dbname_template,
            logs::{
                http_request_builder::{
                    http_healthcheck, GreptimeDBHttpRetryLogic, GreptimeDBLogsHttpRequestBuilder,
                    PartitionKey,
                },
                sink::{GreptimeDBLogsHttpSink, LogsSinkSetting},
            },
            GreptimeDBDefaultBatchSettings,
        },
        prelude::*,
        util::http::HttpService,
    },
};
use std::collections::HashMap;
use vector_lib::{
    codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig},
    configurable::configurable_component,
    sensitive_string::SensitiveString,
};

fn extra_params_examples() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter([("source".to_owned(), "vector".to_owned())])
}

/// Configuration for the `greptimedb_logs` sink.
#[configurable_component(sink("greptimedb_logs", "Ingest logs data into GreptimeDB."))]
#[derive(Clone, Debug, Default, Derivative)]
#[serde(deny_unknown_fields)]
pub struct GreptimeDBLogsConfig {
    /// The endpoint of the GreptimeDB server.
    #[serde(alias = "host")]
    #[configurable(metadata(docs::examples = "http://localhost:4000"))]
    pub endpoint: String,

    /// The table that data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: Template,

    /// The [GreptimeDB database][database] name to connect.
    ///
    /// Default to `public`, the default database of GreptimeDB.
    ///
    /// Database can be created via `create database` statement on
    /// GreptimeDB. If you are using GreptimeCloud, use `dbname` from the
    /// connection information of your instance.
    ///
    /// [database]: https://docs.greptime.com/user-guide/concepts/key-concepts#database
    #[configurable(metadata(docs::examples = "public"))]
    #[derivative(Default(value = "default_dbname_template()"))]
    #[serde(default = "default_dbname_template")]
    pub dbname: Template,

    /// Pipeline name to be used for the logs.
    #[configurable(metadata(docs::examples = "pipeline_name"))]
    pub pipeline_name: Template,

    /// Pipeline version to be used for the logs.
    #[configurable(metadata(docs::examples = "2024-06-07 06:46:23.858293"))]
    pub pipeline_version: Option<Template>,

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
    /// Set http compression encoding for the request
    /// Default to none, `gzip` or `zstd` is supported.
    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    /// Custom parameters to add to the query string for each HTTP request sent to GreptimeDB.
    #[serde(default)]
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::additional_props_description = "A query string parameter."))]
    #[configurable(metadata(docs::examples = "extra_params_examples()"))]
    pub extra_params: Option<HashMap<String, String>>,

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
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;

        let auth = match (self.username.clone(), self.password.clone()) {
            (Some(username), Some(password)) => Some(Auth::Basic {
                user: username,
                password,
            }),
            _ => None,
        };
        let request_builder = GreptimeDBLogsHttpRequestBuilder {
            endpoint: self.endpoint.clone(),
            auth: auth.clone(),
            encoder: (
                self.encoding.clone(),
                Encoder::<Framer>::new(
                    NewlineDelimitedEncoderConfig.build().into(),
                    JsonSerializerConfig::default().build().into(),
                ),
            ),
            compression: self.compression,
            extra_params: self.extra_params.clone(),
        };

        let service: HttpService<GreptimeDBLogsHttpRequestBuilder, PartitionKey> =
            HttpService::new(client.clone(), request_builder.clone());

        let request_limits = self.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, GreptimeDBHttpRetryLogic::default())
            .service(service);

        let logs_sink_setting = LogsSinkSetting {
            dbname: self.dbname.clone(),
            table: self.table.clone(),
            pipeline_name: self.pipeline_name.clone(),
            pipeline_version: self.pipeline_version.clone(),
        };

        let sink = GreptimeDBLogsHttpSink::new(
            self.batch.into_batcher_settings()?,
            service,
            request_builder,
            logs_sink_setting,
        );

        let healthcheck = Box::pin(http_healthcheck(
            client,
            self.endpoint.clone(),
            auth.clone(),
        ));
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
