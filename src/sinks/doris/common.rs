use crate::{
    http::{Auth, MaybeAuth},
    sinks::{
        doris::{
            DorisConfig, client::ThreadSafeDorisSinkClient, request_builder::DorisRequestBuilder,
        },
        prelude::Compression,
        util::UriSerde,
    },
    tls::TlsSettings,
};
use http::Uri;
use vector_lib::codecs::{Encoder, SinkType, encoding::Framer};

#[derive(Debug, Clone)]
pub struct DorisCommon {
    pub base_url: Uri,
    pub auth: Option<Auth>,
    pub request_builder: DorisRequestBuilder,
    pub tls_settings: TlsSettings,
}

impl DorisCommon {
    pub async fn parse_config(config: &DorisConfig, endpoint: &UriSerde) -> crate::Result<Self> {
        if endpoint.uri.host().is_none() {
            return Err(
                format!("Invalid host: {}, host must include hostname", endpoint.uri).into(),
            );
        }

        // basic auth must be some for now
        let auth = config.auth.choose_one(&endpoint.auth)?;
        let base_url = endpoint.uri.clone();
        let tls_settings = TlsSettings::from_options(config.tls.as_ref())?;

        // Build encoder from the encoding configuration
        let transformer = config.encoding.transformer();
        let (framer, serializer) = config.encoding.build(SinkType::StreamBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_builder = DorisRequestBuilder {
            compression: Compression::None,
            encoder: (transformer, encoder),
        };

        Ok(Self {
            base_url,
            auth,
            request_builder,
            tls_settings,
        })
    }
    pub async fn parse_many(config: &DorisConfig) -> crate::Result<Vec<Self>> {
        let mut commons = Vec::new();
        for endpoint in config.endpoints.iter() {
            commons.push(Self::parse_config(config, endpoint).await?);
        }
        Ok(commons)
    }

    pub async fn healthcheck(&self, client: ThreadSafeDorisSinkClient) -> crate::Result<()> {
        client.healthcheck_fenode(&self.base_url).await
    }
}
