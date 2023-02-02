use codecs::encoding::Framer;
use futures::future::FutureExt;
use tower::ServiceBuilder;
use vector_config::{component::GenerateConfig, configurable_component};
use vector_core::tls::TlsSettings;

use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        util::{
            BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings, ServiceBuilderExt,
            TowerRequestConfig, UriSerde,
        },
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
};

use super::{
    api::{DatabendAPIClient, DatabendHttpRequest},
    service::{DatabendRetryLogic, DatabendService},
    sink::{DatabendRequestBuilder, DatabendSink},
};

/// Configuration for the `databend` sink.
#[configurable_component(sink("databend"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DatabendConfig {
    /// The endpoint of the Databend server.
    #[configurable(metadata(docs::examples = "http://localhost:8000"))]
    pub endpoint: UriSerde,

    /// The table that data will be inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: String,

    /// The database that contains the table that data will be inserted into.
    #[configurable(metadata(docs::examples = "mydatabase"))]
    #[serde(default = "DatabendConfig::default_database")]
    pub database: String,

    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for DatabendConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "http://localhost:8000"
            encoding.codec = "json"
            table = "default"
            database = "default"
        "#,
        )
        .unwrap()
    }
}

impl DatabendConfig {
    pub(super) fn build_client(&self, cx: &SinkContext) -> crate::Result<HttpClient> {
        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;
        Ok(client)
    }

    fn default_database() -> String {
        "default".to_string()
    }
}

#[async_trait::async_trait]
impl SinkConfig for DatabendConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let auth = self.auth.choose_one(&self.endpoint.auth)?;
        let endpoint = self.endpoint.with_default_parts();
        let config = DatabendConfig {
            auth: auth.clone(),
            ..self.clone()
        };
        let health_client =
            DatabendAPIClient::new(self.build_client(&cx)?, endpoint.clone(), auth.clone());
        let healthcheck = select_one(health_client).boxed();

        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = self.batch.into_batcher_settings()?;

        let database = config.database;
        let table = config.table;
        let client = DatabendAPIClient::new(self.build_client(&cx)?, endpoint, auth);
        let service = DatabendService::new(client, database, table)?;
        let service = ServiceBuilder::new()
            .settings(request_settings, DatabendRetryLogic)
            .service(service);

        let transformer = self.encoding.transformer();
        let (framer, serializer) = self.encoding.build(SinkType::StreamBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_builder =
            DatabendRequestBuilder::new(config.compression, (transformer, encoder));

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
    let req = DatabendHttpRequest::new("SELECT 1".to_string());
    client.query(req).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use codecs::encoding::Serializer;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatabendConfig>();
    }

    #[test]
    fn parse_database() {
        let cfg = toml::from_str::<DatabendConfig>(
            r#"
            endpoint = "http://localhost:8000"
            table = "mytable"
            database = "mydatabase"
            encoding.codec = "json"
        "#,
        )
        .unwrap();
        assert_eq!(cfg.endpoint.uri, "http://localhost:8000");
        assert_eq!(cfg.table, "mytable");
        assert_eq!(cfg.database, "mydatabase");

        let (framer, serializer) = cfg.encoding.build(SinkType::StreamBased).unwrap();
        match framer {
            Framer::NewlineDelimited(_) => (),
            _ => panic!("Unexpected framer"),
        }
        match serializer {
            Serializer::Json(_) => (),
            _ => panic!("Unexpected serializer"),
        }
    }
}
