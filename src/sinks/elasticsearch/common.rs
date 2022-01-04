use std::{collections::HashMap, convert::TryFrom};

use http::{StatusCode, Uri};
use hyper::Body;
use rusoto_core::Region;
use rusoto_signature::SignedRequest;
use snafu::ResultExt;

use super::{InvalidHost, Request};
use crate::{
    aws::{rusoto, rusoto::region_from_endpoint},
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        elasticsearch::{
            encoder::ElasticSearchEncoder, finish_signer, ElasticSearchAuth,
            ElasticSearchCommonMode, ElasticSearchConfig, ParseError,
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
pub struct ElasticSearchCommon {
    pub base_url: String,
    pub id_key: Option<String>,
    pub bulk_uri: Uri,
    pub authorization: Option<Auth>,
    pub credentials: Option<rusoto::AwsCredentialsProvider>,
    pub encoding: EncodingConfigFixed<ElasticSearchEncoder>,
    pub mode: ElasticSearchCommonMode,
    pub doc_type: String,
    pub suppress_type_name: bool,
    pub tls_settings: TlsSettings,
    pub compression: Compression,
    pub region: Region,
    pub request: RequestConfig,
    pub query_params: HashMap<String, String>,
    pub metric_to_log: MetricToLog,
}

impl ElasticSearchCommon {
    pub fn parse_config(config: &ElasticSearchConfig) -> crate::Result<Self> {
        // Test the configured host, but ignore the result
        let uri = format!("{}/_test", &config.endpoint);
        let uri = uri.parse::<Uri>().with_context(|| InvalidHost {
            host: &config.endpoint,
        })?;
        if uri.host().is_none() {
            return Err(ParseError::HostMustIncludeHostname {
                host: config.endpoint.clone(),
            }
            .into());
        }

        let authorization = match &config.auth {
            Some(ElasticSearchAuth::Basic { user, password }) => Some(Auth::Basic {
                user: user.clone(),
                password: password.clone(),
            }),
            _ => None,
        };
        let uri = config.endpoint.parse::<UriSerde>()?;
        let authorization = authorization.choose_one(&uri.auth)?;
        let base_url = uri.uri.to_string().trim_end_matches('/').to_owned();

        let region = match &config.aws {
            Some(region) => Region::try_from(region)?,
            None => region_from_endpoint(&base_url)?,
        };

        let credentials = match &config.auth {
            Some(ElasticSearchAuth::Basic { .. }) | None => None,
            Some(ElasticSearchAuth::Aws(aws)) => Some(aws.build(&region, None)?),
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

        Ok(Self {
            authorization,
            base_url,
            bulk_uri,
            compression,
            credentials,
            doc_type,
            suppress_type_name: config.suppress_type_name,
            encoding: config.encoding,
            id_key: config.id_key,
            mode,
            query_params,
            request,
            region,
            tls_settings,
            metric_to_log,
        })
    }

    pub fn signed_request(&self, method: &str, uri: &Uri, use_params: bool) -> SignedRequest {
        let mut request = SignedRequest::new(method, "es", &self.region, uri.path());
        request.set_hostname(uri.host().map(|host| host.into()));
        if use_params {
            for (key, value) in &self.query_params {
                request.add_param(key, value);
            }
        }
        request
    }

    pub async fn healthcheck(self, client: HttpClient) -> crate::Result<()> {
        let mut builder = Request::get(format!("{}/_cluster/health", self.base_url));

        match &self.credentials {
            None => {
                if let Some(authorization) = &self.authorization {
                    builder = authorization.apply_builder(builder);
                }
            }
            Some(credentials_provider) => {
                let mut signer = self.signed_request("GET", builder.uri_ref().unwrap(), false);
                builder = finish_signer(&mut signer, credentials_provider, builder).await?;
            }
        }
        let request = builder.body(Body::empty())?;
        let response = client.send(request).await?;

        match response.status() {
            StatusCode::OK => Ok(()),
            status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
        }
    }
}
