#![expect(
    clippy::let_underscore_must_use,
    reason = "derivative's Debug derive with ignored fields expands to a must_use let binding"
)]

use std::{collections::BTreeMap, sync::Arc};

use databend_client::APIClient as DatabendAPIClient;
use derivative::Derivative;
use futures::future::FutureExt;
use tower::ServiceBuilder;
use vector_lib::{
    codecs::{
        Transformer,
        encoding::{Framer, FramingConfig},
    },
    configurable::{component::GenerateConfig, configurable_component},
    stream::BatcherSettings,
};

use super::{
    compression::DatabendCompression,
    encoding::{DatabendEncodingConfig, DatabendMissingFieldAS, DatabendSerializerConfig},
    request_builder::DatabendRequestBuilder,
    service::{DatabendRetryLogic, DatabendService},
    sink::DatabendSink,
};
use crate::{
    codecs::{Encoder, EncodingConfig},
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext, ValidatedSink},
    http::{Auth, MaybeAuth},
    sinks::{
        Healthcheck, VectorSink,
        util::{
            BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings, ServiceBuilderExt,
            TowerRequestConfig, TowerRequestSettings, UriSerde,
        },
    },
    tls::TlsConfig,
    vector_version,
};

/// Configuration for the `databend` sink.
#[configurable_component(sink("databend", "Deliver log data to a Databend database."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DatabendConfig {
    /// The DSN of the Databend server.
    #[configurable(metadata(
        docs::examples = "databend://localhost:8000/default?sslmode=disable"
    ))]
    pub endpoint: UriSerde,

    /// The TLS configuration to use when connecting to the Databend server.
    #[configurable(
        deprecated = "This option has been deprecated, use arguments in the DSN instead."
    )]
    pub tls: Option<TlsConfig>,

    /// The database that contains the table that data is inserted into. Overrides the database in DSN.
    #[configurable(metadata(docs::examples = "mydatabase"))]
    pub database: Option<String>,

    /// The username and password to authenticate with. Overrides the username and password in DSN.
    pub auth: Option<Auth>,

    /// The table that data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: String,

    #[serde(default)]
    pub missing_field_as: DatabendMissingFieldAS,

    #[serde(default)]
    pub encoding: DatabendEncodingConfig,

    #[serde(default)]
    pub compression: DatabendCompression,

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

impl GenerateConfig for DatabendConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"endpoint: "databend://localhost:8000/default?sslmode=disable"
            table: default
        "#,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "databend")]
impl SinkConfig for DatabendConfig {
    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ValidatedDatabend {
    // Omitted: `endpoint` embeds the basic-auth username/password in its URL.
    #[derivative(Debug = "ignore")]
    endpoint: String,
    request_settings: TowerRequestSettings,
    batch_settings: BatcherSettings,
    file_format_options: BTreeMap<&'static str, &'static str>,
    compression: Compression,
    transformer: Transformer,
}

#[async_trait::async_trait]
impl ValidatedSink for DatabendConfig {
    type Validated = ValidatedDatabend;

    fn validate(&self) -> crate::Result<ValidatedDatabend> {
        // `DatabendService::new` rejects an empty table at build time, so
        // reject it here to keep `vector validate --no-environment` from
        // accepting a sink that cannot start.
        if self.table.is_empty() {
            return Err("`table` is required".into());
        }
        let auth = self.auth.choose_one(&self.endpoint.auth)?;
        let authority = self
            .endpoint
            .uri
            .authority()
            .ok_or("Endpoint missing authority")?;
        let endpoint = match self.endpoint.uri.scheme().map(|s| s.as_str()) {
            Some("databend") => self.endpoint.to_string(),
            // for backward compatibility, build DSN from endpoint
            Some("http") => format!("databend://{authority}/?sslmode=disable"),
            Some("https") => format!("databend://{authority}"),
            None => {
                return Err("Missing scheme for Databend endpoint. Expected `databend`.".into());
            }
            Some(s) => {
                return Err(format!("Unsupported scheme for Databend endpoint: {s}").into());
            }
        };
        let mut endpoint = url::Url::parse(&endpoint)?;
        match auth {
            Some(Auth::Basic { user, password }) => {
                // Only fails for host-less URLs, which cannot happen given the scheme validation above.
                endpoint.set_username(&user).ok();
                endpoint.set_password(Some(password.inner())).ok();
            }
            Some(Auth::Bearer { .. }) => {
                return Err("Bearer authentication is not supported currently".into());
            }
            Some(Auth::Custom { .. }) => {
                return Err("Custom authentication is not supported currently".into());
            }
            None => {}
            #[cfg(feature = "aws-core")]
            _ => {}
        }
        if let Some(database) = &self.database {
            endpoint.set_path(&format!("/{database}"));
        }
        let endpoint = endpoint.to_string();

        let request_settings = self.request.into_settings();
        let batch_settings = self.batch.into_batcher_settings()?;

        let mut file_format_options = BTreeMap::new();
        let compression = match self.compression {
            DatabendCompression::Gzip => {
                file_format_options.insert("compression", "GZIP");
                Compression::gzip_default()
            }
            DatabendCompression::None => {
                file_format_options.insert("compression", "NONE");
                Compression::None
            }
        };
        let encoding: EncodingConfig = self.encoding.clone().into();
        match self.encoding.config() {
            DatabendSerializerConfig::Json(_) => {
                file_format_options.insert("type", "NDJSON");
                file_format_options.insert("missing_field_as", self.missing_field_as.as_str());
            }
            DatabendSerializerConfig::Csv(_) => {
                file_format_options.insert("type", "CSV");
                file_format_options.insert("field_delimiter", ",");
                file_format_options.insert("record_delimiter", "\n");
                file_format_options.insert("skip_header", "0");
            }
        }
        let transformer = encoding.transformer();

        Ok(ValidatedDatabend {
            endpoint,
            request_settings,
            batch_settings,
            file_format_options,
            compression,
            transformer,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedDatabend,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedDatabend {
            endpoint,
            request_settings,
            batch_settings,
            file_format_options,
            compression,
            transformer,
        } = validated;

        let ua = format!("vector/{}", vector_version());
        let health_client = DatabendAPIClient::new(endpoint, Some(ua.clone())).await?;
        let healthcheck = select_one(health_client).boxed();

        let mut copy_options = BTreeMap::new();
        copy_options.insert("purge", "true");

        let client = DatabendAPIClient::new(endpoint, Some(ua)).await?;
        let service = DatabendService::new(
            client,
            self.table.clone(),
            file_format_options.clone(),
            copy_options,
        )?;
        let service = ServiceBuilder::new()
            .settings(request_settings.clone(), DatabendRetryLogic)
            .service(service);

        let encoding: EncodingConfig = self.encoding.clone().into();
        let serializer = encoding.build()?;
        let framer = FramingConfig::NewlineDelimited.build();
        let encoder = Encoder::<Framer>::new(framer, serializer);
        let request_builder =
            DatabendRequestBuilder::new(*compression, (transformer.clone(), encoder));

        let sink = DatabendSink::new(*batch_settings, request_builder, service);

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

async fn select_one(client: Arc<DatabendAPIClient>) -> crate::Result<()> {
    client.query_all("SELECT 1").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatabendConfig>();
    }

    #[test]
    fn validate_produces_usable_values() {
        use crate::config::ValidatedSink;
        let config = serde_yaml::from_str::<DatabendConfig>(indoc::indoc! {r#"
            endpoint: "databend://localhost:8000/mydatabase?sslmode=disable"
            table: "mytable"
        "#})
        .unwrap();
        let validated = config.validate().expect("validation should succeed");
        assert!(validated.endpoint.starts_with("databend://localhost:8000"));
        assert!(matches!(validated.compression, Compression::None));
    }

    #[test]
    fn validate_rejects_empty_table() {
        use crate::config::ValidatedSink;
        let config = serde_yaml::from_str::<DatabendConfig>(indoc::indoc! {r#"
            endpoint: "databend://localhost:8000/mydatabase?sslmode=disable"
            table: ""
        "#})
        .unwrap();
        let err = config.validate().unwrap_err().to_string();
        assert!(
            err.contains("`table` is required"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_config() {
        let cfg = serde_yaml::from_str::<DatabendConfig>(indoc::indoc! {r#"
            endpoint: "databend://localhost:8000/mydatabase?sslmode=disable"
            table: "mytable"
        "#})
        .unwrap();
        assert_eq!(
            cfg.endpoint.uri,
            "databend://localhost:8000/mydatabase?sslmode=disable"
        );
        assert_eq!(cfg.table, "mytable");
        assert!(matches!(
            cfg.encoding.config(),
            DatabendSerializerConfig::Json(_)
        ));
        assert!(matches!(cfg.compression, DatabendCompression::None));
    }

    #[test]
    fn parse_config_with_encoding_compression() {
        let cfg = serde_yaml::from_str::<DatabendConfig>(indoc::indoc! {r#"
            endpoint: "databend://localhost:8000/mydatabase?sslmode=disable"
            table: "mytable"
            encoding:
              codec: "csv"
              csv:
                fields:
                  - "host"
                  - "timestamp"
                  - "message"
            compression: "gzip"
        "#})
        .unwrap();
        assert_eq!(
            cfg.endpoint.uri,
            "databend://localhost:8000/mydatabase?sslmode=disable"
        );
        assert_eq!(cfg.table, "mytable");
        assert!(matches!(
            cfg.encoding.config(),
            DatabendSerializerConfig::Csv(_)
        ));
        assert!(matches!(cfg.compression, DatabendCompression::Gzip));
    }
}
