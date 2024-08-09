use std::collections::BTreeMap;

use databend_client::APIClient as DatabendAPIClient;
use futures::future::FutureExt;
use tower::ServiceBuilder;
use vector_lib::codecs::encoding::{Framer, FramingConfig};
use vector_lib::configurable::{component::GenerateConfig, configurable_component};

use crate::{
    codecs::{Encoder, EncodingConfig},
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    http::{Auth, MaybeAuth},
    sinks::{
        util::{
            BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings, ServiceBuilderExt,
            TowerRequestConfig, UriSerde,
        },
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
    vector_version,
};

use super::{
    compression::DatabendCompression,
    encoding::{DatabendEncodingConfig, DatabendMissingFieldAS, DatabendSerializerConfig},
    request_builder::DatabendRequestBuilder,
    service::{DatabendRetryLogic, DatabendService},
    sink::DatabendSink,
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
    #[configurable(derived)]
    pub auth: Option<Auth>,

    /// The table that data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: String,

    #[configurable(derived)]
    #[serde(default)]
    pub missing_field_as: DatabendMissingFieldAS,

    #[configurable(derived)]
    #[serde(default)]
    pub encoding: DatabendEncodingConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: DatabendCompression,

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

impl GenerateConfig for DatabendConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "databend://localhost:8000/default?sslmode=disable"
            table = "default"
        "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "databend")]
impl SinkConfig for DatabendConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let ua = format!("vector/{}", vector_version());
        let auth = self.auth.choose_one(&self.endpoint.auth)?;
        let authority = self
            .endpoint
            .uri
            .authority()
            .ok_or("Endpoint missing authority")?;
        let endpoint = match self.endpoint.uri.scheme().map(|s| s.as_str()) {
            Some("databend") => self.endpoint.to_string(),
            // for backward compatibility, build DSN from endpoint
            Some("http") => format!("databend://{}/?sslmode=disable", authority),
            Some("https") => format!("databend://{}", authority),
            None => {
                return Err("Missing scheme for Databend endpoint. Expected `databend`.".into());
            }
            Some(s) => {
                return Err(format!("Unsupported scheme for Databend endpoint: {}", s).into());
            }
        };
        let mut endpoint = url::Url::parse(&endpoint)?;
        match auth {
            Some(Auth::Basic { user, password }) => {
                let _ = endpoint.set_username(&user);
                let _ = endpoint.set_password(Some(password.inner()));
            }
            Some(Auth::Bearer { .. }) => {
                return Err("Bearer authentication is not supported currently".into());
            }
            Some(Auth::OAuth2 { .. }) => {
                todo!()
            }
            None => {}
        }
        if let Some(database) = &self.database {
            endpoint.set_path(&format!("/{}", database));
        }
        let endpoint = endpoint.to_string();
        let health_client = DatabendAPIClient::new(&endpoint, Some(ua.clone())).await?;
        let healthcheck = select_one(health_client).boxed();

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
        let serializer = match self.encoding.config() {
            DatabendSerializerConfig::Json(_) => {
                file_format_options.insert("type", "NDJSON");
                file_format_options.insert("missing_field_as", self.missing_field_as.as_str());
                encoding.build()?
            }
            DatabendSerializerConfig::Csv(_) => {
                file_format_options.insert("type", "CSV");
                file_format_options.insert("field_delimiter", ",");
                file_format_options.insert("record_delimiter", "\n");
                file_format_options.insert("skip_header", "0");
                encoding.build()?
            }
        };
        let framer = FramingConfig::NewlineDelimited.build();
        let transformer = encoding.transformer();

        let mut copy_options = BTreeMap::new();
        copy_options.insert("purge", "true");

        let client = DatabendAPIClient::new(&endpoint, Some(ua)).await?;
        let service = DatabendService::new(
            client,
            self.table.clone(),
            file_format_options,
            copy_options,
        )?;
        let service = ServiceBuilder::new()
            .settings(request_settings, DatabendRetryLogic)
            .service(service);

        let encoder = Encoder::<Framer>::new(framer, serializer);
        let request_builder = DatabendRequestBuilder::new(compression, (transformer, encoder));

        let sink = DatabendSink::new(batch_settings, request_builder, service);

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

async fn select_one(client: DatabendAPIClient) -> crate::Result<()> {
    client.query("SELECT 1").await?;
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
    fn parse_config() {
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "databend://localhost:8000/mydatabase?sslmode=disable"
            table = "mytable"
        "#,
        )
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
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "databend://localhost:8000/mydatabase?sslmode=disable"
            table = "mytable"
            encoding.codec = "csv"
            encoding.csv.fields = ["host", "timestamp", "message"]
            compression = "gzip"
        "#,
        )
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
