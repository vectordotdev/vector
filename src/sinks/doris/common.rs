use crate::{
    codecs::Encoder,
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
use snafu::prelude::*;
use vector_lib::codecs::encoding::Framer;

#[derive(Debug, Snafu)]
pub enum ParseError {
    #[snafu(display("Invalid host: {}, host must include hostname", host))]
    HostMustIncludeHostname { host: String },
}

#[derive(Debug, Snafu)]
pub struct InvalidHostError {
    host: String,
    source: http::uri::InvalidUri,
}

#[derive(Debug, Clone)]
pub struct DorisCommon {
    pub base_url: String,
    pub auth: Option<Auth>,
    pub request_builder: DorisRequestBuilder,
    pub tls_settings: TlsSettings,
}

impl DorisCommon {
    pub async fn parse_config(config: &DorisConfig, endpoint: &str) -> crate::Result<Self> {
        let uri = format!("{}/_test", endpoint);
        let uri = uri
            .parse::<Uri>()
            .context(InvalidHostSnafu { host: endpoint })?;
        if uri.host().is_none() {
            return Err(ParseError::HostMustIncludeHostname {
                host: endpoint.to_string(),
            }
            .into());
        }
        let uri = endpoint.parse::<UriSerde>()?;

        // basic auth must be some for now
        let auth = config.auth.choose_one(&uri.auth)?;
        let base_url = uri.uri.to_string().trim_end_matches('/').to_owned();
        let tls_settings = TlsSettings::from_options(config.tls.as_ref())?;

        // Build encoder from the encoding configuration
        let transformer = config.encoding.transformer();
        let (framer, serializer) = config
            .encoding
            .build(crate::codecs::SinkType::StreamBased)?;
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
        client.healthcheck_fenode(self.base_url.clone()).await
    }
}
