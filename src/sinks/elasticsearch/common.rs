use aws_sigv4::http_request::{SignableRequest, SigningSettings};
use aws_sigv4::SigningParams;
use aws_types::credentials::{ProvideCredentials, SharedCredentialsProvider};
use aws_types::region::Region;
use bytes::Bytes;
use std::collections::HashMap;
use std::time::SystemTime;

use http::{StatusCode, Uri};
use snafu::ResultExt;

use super::{InvalidHostSnafu, Request};
use crate::{
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        elasticsearch::{
            encoder::ElasticsearchEncoder, ElasticsearchAuth, ElasticsearchCommonMode,
            ElasticsearchConfig, ParseError,
        },
        util::{
            encoding::EncodingConfigFixed, http::RequestConfig, Compression, TowerRequestConfig,
            UriSerde,
        },
        HealthcheckError,
    },
    tls::TlsSettings,
    transforms::metric_to_log::MetricToLog,
};

#[derive(Debug)]
pub struct ElasticsearchCommon {
    pub base_url: String,
    pub bulk_uri: Uri,
    pub http_auth: Option<Auth>,
    pub aws_auth: Option<SharedCredentialsProvider>,
    pub encoding: EncodingConfigFixed<ElasticsearchEncoder>,
    pub mode: ElasticsearchCommonMode,
    pub doc_type: String,
    pub suppress_type_name: bool,
    pub tls_settings: TlsSettings,
    pub compression: Compression,
    pub region: Option<Region>,
    pub request: RequestConfig,
    pub query_params: HashMap<String, String>,
    pub metric_to_log: MetricToLog,
}

impl ElasticsearchCommon {
    pub async fn parse_config(config: &ElasticsearchConfig) -> crate::Result<Self> {
        // Test the configured host, but ignore the result
        let uri = format!("{}/_test", &config.endpoint);
        let uri = uri.parse::<Uri>().with_context(|_| InvalidHostSnafu {
            host: &config.endpoint,
        })?;
        if uri.host().is_none() {
            return Err(ParseError::HostMustIncludeHostname {
                host: config.endpoint.clone(),
            }
            .into());
        }

        let authorization = match &config.auth {
            Some(ElasticsearchAuth::Basic { user, password }) => Some(Auth::Basic {
                user: user.clone(),
                password: password.clone(),
            }),
            _ => None,
        };
        let uri = config.endpoint.parse::<UriSerde>()?;
        let http_auth = authorization.choose_one(&uri.auth)?;
        let base_url = uri.uri.to_string().trim_end_matches('/').to_owned();

        let aws_auth = match &config.auth {
            Some(ElasticsearchAuth::Basic { .. }) | None => None,
            Some(ElasticsearchAuth::Aws(aws)) => {
                let region = config
                    .aws
                    .as_ref()
                    .map(|config| config.region())
                    .ok_or(ParseError::RegionRequired)?;

                Some(aws.credentials_provider(region).await?)
            }
        };

        let compression = config.compression;
        let mode = config.common_mode()?;

        let doc_type = config.doc_type.clone().unwrap_or_else(|| "_doc".into());

        let tower_request = config
            .request
            .tower
            .unwrap_with(&TowerRequestConfig::default());

        let mut query_params = config.query.clone().unwrap_or_default();
        query_params.insert(
            "timeout".into(),
            format!("{}s", tower_request.timeout.as_secs()),
        );

        if let Some(pipeline) = &config.pipeline {
            query_params.insert("pipeline".into(), pipeline.into());
        }

        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (p, v) in &query_params {
            query.append_pair(&p[..], &v[..]);
        }
        let bulk_url = format!("{}/_bulk?{}", base_url, query.finish());
        let bulk_uri = bulk_url.parse::<Uri>().unwrap();

        let tls_settings = TlsSettings::from_options(&config.tls)?;
        let config = config.clone();
        let request = config.request;

        let metric_config = config.metrics.clone().unwrap_or_default();
        let metric_to_log = MetricToLog::new(
            metric_config.host_tag,
            metric_config.timezone.unwrap_or_default(),
        );

        let region = config.aws.as_ref().map(|config| config.region());

        Ok(Self {
            http_auth,
            base_url,
            bulk_uri,
            compression,
            aws_auth,
            doc_type,
            suppress_type_name: config.suppress_type_name,
            encoding: config.encoding,
            mode,
            query_params,
            request,
            region,
            tls_settings,
            metric_to_log,
        })
    }

    pub async fn healthcheck(self, client: HttpClient) -> crate::Result<()> {
        let mut builder = Request::get(format!("{}/_cluster/health", self.base_url));

        if let Some(authorization) = &self.http_auth {
            builder = authorization.apply_builder(builder);
        }
        let mut request = builder.body(Bytes::new())?;

        if let Some(credentials_provider) = &self.aws_auth {
            sign_request(&mut request, credentials_provider, &self.region).await?;
        }
        let response = client.send(request.map(hyper::Body::from)).await?;

        match response.status() {
            StatusCode::OK => Ok(()),
            status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
        }
    }
}

pub async fn sign_request(
    request: &mut http::Request<Bytes>,
    credentials_provider: &SharedCredentialsProvider,
    region: &Option<Region>,
) -> crate::Result<()> {
    let signable_request = SignableRequest::from(&*request);
    let credentials = credentials_provider.provide_credentials().await?;
    let mut signing_params_builder = SigningParams::builder()
        .access_key(credentials.access_key_id())
        .secret_key(credentials.secret_access_key())
        .region(region.as_ref().map(|r| r.as_ref()).unwrap_or(""))
        .service_name("es")
        .time(SystemTime::now())
        .settings(SigningSettings::default());

    signing_params_builder.set_security_token(credentials.session_token());

    let (signing_instructions, _signature) =
        aws_sigv4::http_request::sign(signable_request, &signing_params_builder.build()?)?
            .into_parts();
    signing_instructions.apply_to_request(request);

    Ok(())
}
