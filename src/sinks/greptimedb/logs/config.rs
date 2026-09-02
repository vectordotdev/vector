use std::collections::HashMap;

use vector_lib::{
    codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig, encoding::Framer},
    configurable::configurable_component,
    sensitive_string::SensitiveString,
};

use crate::{
    config::ValidatedSink,
    http::{Auth, HttpClient},
    sinks::{
        greptimedb::{
            GreptimeDBDefaultBatchSettings, default_dbname_template, default_pipeline_template,
            logs::{
                http_request_builder::{
                    GreptimeDBHttpRetryLogic, GreptimeDBLogsHttpRequestBuilder, PartitionKey,
                    http_healthcheck,
                },
                sink::{GreptimeDBLogsHttpSink, LogsSinkSetting},
            },
        },
        prelude::*,
        util::{HttpEndpoint, http::HttpService},
    },
    template::ConfinementConfig,
};

fn extra_params_examples() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter([("source".to_owned(), "vector".to_owned())])
}

fn default_endpoint() -> HttpEndpoint {
    HttpEndpoint::parse("http://localhost:4000")
        .expect("static default endpoint should be a valid http(s) URL")
}

/// Configuration for the `greptimedb_logs` sink.
#[configurable_component(sink("greptimedb_logs", "Ingest logs data into GreptimeDB."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct GreptimeDBLogsConfig {
    /// The endpoint of the GreptimeDB server.
    #[serde(alias = "host")]
    #[derivative(Default(value = "default_endpoint()"))]
    #[configurable(metadata(docs::examples = "http://localhost:4000"))]
    pub endpoint: HttpEndpoint,

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
    ///
    /// Default to `greptime_identity`, use the original log structure
    #[configurable(metadata(docs::examples = "pipeline_name"))]
    #[derivative(Default(value = "default_pipeline_template()"))]
    #[serde(default = "default_pipeline_template")]
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
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    /// Custom parameters to add to the query string for each HTTP request sent to GreptimeDB.
    #[serde(default)]
    #[configurable(metadata(docs::additional_props_description = "A query string parameter."))]
    #[configurable(metadata(docs::examples = "extra_params_examples()"))]
    pub extra_params: Option<HashMap<String, String>>,

    /// Custom headers to add to the HTTP request sent to GreptimeDB.
    /// Note that these headers will override the existing headers.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "Extra header key-value pairs."
    ))]
    pub extra_headers: Option<HashMap<String, String>>,

    #[serde(default)]
    pub(crate) batch: BatchConfig<GreptimeDBDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    pub tls: Option<TlsConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

impl_generate_config_from_default!(GreptimeDBLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "greptimedb_logs")]
impl SinkConfig for GreptimeDBLogsConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedGreptimeDBLogs {
    confined_table: ConfinedTemplate,
    confined_dbname: ConfinedTemplate,
    confined_pipeline_name: ConfinedTemplate,
    confined_pipeline_version: Option<ConfinedTemplate>,
    auth: Option<Auth>,
    batch_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for GreptimeDBLogsConfig {
    type Validated = ValidatedGreptimeDBLogs;

    fn validate(&self) -> crate::Result<ValidatedGreptimeDBLogs> {
        let confined_table = self
            .table
            .clone()
            .confine(&self.confinement, Self::NAME, "table")?;
        let confined_dbname =
            self.dbname
                .clone()
                .confine(&self.confinement, Self::NAME, "dbname")?;
        let confined_pipeline_name =
            self.pipeline_name
                .clone()
                .confine(&self.confinement, Self::NAME, "pipeline_name")?;
        let confined_pipeline_version = self
            .pipeline_version
            .clone()
            .map(|t| t.confine(&self.confinement, Self::NAME, "pipeline_version"))
            .transpose()?;

        let auth = match (self.username.clone(), self.password.clone()) {
            (Some(username), Some(password)) => Some(Auth::Basic {
                user: username,
                password,
            }),
            _ => None,
        };

        let batch_settings = self.batch.into_batcher_settings()?;

        Ok(ValidatedGreptimeDBLogs {
            confined_table,
            confined_dbname,
            confined_pipeline_name,
            confined_pipeline_version,
            auth,
            batch_settings,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedGreptimeDBLogs,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedGreptimeDBLogs {
            confined_table,
            confined_dbname,
            confined_pipeline_name,
            confined_pipeline_version,
            auth,
            batch_settings,
        } = validated;

        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;

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
            extra_headers: self.extra_headers.clone(),
        };

        let service: HttpService<GreptimeDBLogsHttpRequestBuilder, PartitionKey> =
            HttpService::new(client.clone(), request_builder.clone());

        let request_limits = self.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, GreptimeDBHttpRetryLogic::default())
            .service(service);

        let logs_sink_setting = LogsSinkSetting {
            dbname: confined_dbname.clone(),
            table: confined_table.clone(),
            pipeline_name: confined_pipeline_name.clone(),
            pipeline_version: confined_pipeline_version.clone(),
        };

        let sink = GreptimeDBLogsHttpSink::new(
            *batch_settings,
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
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;
    use crate::{
        config::ValidatedSink,
        template::{ConfinementConfig, Template},
    };

    #[test]
    fn prepares_valid_config() {
        let config = GreptimeDBLogsConfig {
            endpoint: HttpEndpoint::parse("http://localhost:4000").unwrap(),
            table: "mytable".try_into().unwrap(),
            dbname: "public".try_into().unwrap(),
            pipeline_name: "greptime_identity".try_into().unwrap(),
            ..Default::default()
        };

        let validated = config.validate().expect("preparation should succeed");
        assert_eq!(validated.confined_table.to_string(), "mytable");
        assert_eq!(validated.confined_dbname.to_string(), "public");
        assert_eq!(
            validated.confined_pipeline_name.to_string(),
            "greptime_identity"
        );
        assert!(validated.auth.is_none());
    }

    #[test]
    fn validate_rejects_malformed_endpoint() {
        // `HttpEndpoint` rejects a malformed endpoint at load time, so
        // deserialization fails.
        let result: Result<GreptimeDBLogsConfig, _> = serde_yaml::from_str(indoc! {r#"
            endpoint: "not a uri"
            table: "mytable"
        "#});
        assert!(
            result.is_err(),
            "config load should reject a malformed endpoint"
        );
    }

    #[test]
    fn validate_rejects_non_http_endpoint() {
        // `HttpEndpoint` only accepts absolute http(s) URLs, so an `ftp://`
        // endpoint is rejected at load time.
        let result: Result<GreptimeDBLogsConfig, _> = serde_yaml::from_str(indoc! {r#"
            endpoint: "ftp://example.com"
            table: "mytable"
        "#});
        assert!(
            result.is_err(),
            "config load should reject a non-http endpoint"
        );
    }

    #[test]
    fn confinement_rejects_unconfined_table() {
        let template = Template::try_from("{{ table }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "greptimedb_logs", "table");
        assert!(result.is_err());
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_table() {
        let template = Template::try_from("{{ table }}").unwrap();
        let config = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let result = template.confine(&config, "greptimedb_logs", "table");
        assert!(result.is_ok());
    }

    #[test]
    fn confinement_allows_prefixed_table() {
        let template = Template::try_from("events-{{ env }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "greptimedb_logs", "table");
        assert!(result.is_ok());
    }
}
