use std::collections::HashMap;

use aws_types::credentials::SharedCredentialsProvider;
use aws_types::region::Region;
use bytes::{Buf, Bytes};
use http::{Response, StatusCode, Uri};
use hyper::{body, Body};
use serde::Deserialize;
use snafu::ResultExt;

use super::{ElasticsearchApiVersion, InvalidHostSnafu, Request};
use crate::{
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        elasticsearch::{
            ElasticsearchAuth, ElasticsearchCommonMode, ElasticsearchConfig, ParseError,
        },
        util::{http::RequestConfig, TowerRequestConfig, UriSerde},
        HealthcheckError,
    },
    tls::TlsSettings,
    transforms::metric_to_log::MetricToLog,
};

#[derive(Debug, Clone)]
pub struct ElasticsearchCommon {
    pub base_url: String,
    pub bulk_uri: Uri,
    pub http_auth: Option<Auth>,
    pub aws_auth: Option<SharedCredentialsProvider>,
    pub mode: ElasticsearchCommonMode,
    pub tls_settings: TlsSettings,
    pub region: Option<Region>,
    pub request: RequestConfig,
    pub query_params: HashMap<String, String>,
    pub metric_to_log: MetricToLog,
    pub api_version: ElasticsearchApiVersion,
}

impl ElasticsearchCommon {
    pub async fn parse_config(config: &ElasticsearchConfig, endpoint: &str) -> crate::Result<Self> {
        // Test the configured host, but ignore the result
        let uri = format!("{}/_test", endpoint);
        let uri = uri
            .parse::<Uri>()
            .with_context(|_| InvalidHostSnafu { host: endpoint })?;
        if uri.host().is_none() {
            return Err(ParseError::HostMustIncludeHostname {
                host: endpoint.to_string(),
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
        let uri = endpoint.parse::<UriSerde>()?;
        let http_auth = authorization.choose_one(&uri.auth)?;
        let base_url = uri.uri.to_string().trim_end_matches('/').to_owned();

        let aws_auth = match &config.auth {
            Some(ElasticsearchAuth::Basic { .. }) | None => None,
            Some(ElasticsearchAuth::Aws(aws)) => {
                let region = config
                    .aws
                    .as_ref()
                    .map(|config| config.region())
                    .ok_or(ParseError::RegionRequired)?
                    .ok_or(ParseError::RegionRequired)?;

                Some(aws.credentials_provider(region).await?)
            }
        };

        let mode = config.common_mode()?;

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

        let region = config.aws.as_ref().and_then(|config| config.region());

        Ok(Self {
            http_auth,
            base_url,
            bulk_uri,
            aws_auth,
            mode,
            query_params,
            request,
            region,
            tls_settings,
            metric_to_log,
            api_version: config.api_version.clone(),
        })
    }

    /// Parses endpoints into a vector of ElasticsearchCommons. The resulting vector is guaranteed to not be empty.
    pub async fn parse_many(config: &ElasticsearchConfig) -> crate::Result<Vec<Self>> {
        if let Some(endpoint) = config.endpoint.as_ref() {
            warn!(message = "DEPRECATION, use of deprecated option `endpoint`. Please use `endpoints` option instead.");
            if config.endpoints.is_empty() {
                Ok(vec![Self::parse_config(config, endpoint).await?])
            } else {
                Err(ParseError::EndpointsExclusive.into())
            }
        } else if config.endpoints.is_empty() {
            Err(ParseError::EndpointRequired.into())
        } else {
            let mut commons = Vec::new();
            for endpoint in config.endpoints.iter() {
                commons.push(Self::parse_config(config, endpoint).await?);
            }
            Ok(commons)
        }
    }

    /// Parses a single endpoint, else panics.
    #[cfg(test)]
    pub async fn parse_single(config: &ElasticsearchConfig) -> crate::Result<Self> {
        let mut commons = Self::parse_many(config).await?;
        assert!(commons.len() == 1);
        Ok(commons.remove(0))
    }

    pub async fn healthcheck(self, client: HttpClient) -> crate::Result<()> {
        match self.get(client, "/_cluster/health").await?.status() {
            StatusCode::OK => Ok(()),
            status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
        }
    }

    /// Returns major api version. May fetch from Elasticsearch.
    pub async fn api_version(&self, client: &HttpClient) -> crate::Result<usize> {
        match self.api_version {
            ElasticsearchApiVersion::V6 => Ok(6),
            ElasticsearchApiVersion::V7 => Ok(7),
            ElasticsearchApiVersion::V8 => Ok(8),
            ElasticsearchApiVersion::Auto => self
                .clone()
                .get_api_version(client.clone())
                .await
                .map_err(|error| {
                    format!("Failed to get Elasticsearch API version: {}", error).into()
                }),
        }
    }

    /// Fetches version from Elasticsearch.
    async fn get_api_version(self, client: HttpClient) -> crate::Result<usize> {
        let response = self.get(client, "/_cluster/state/version").await?;

        let (_, body) = response.into_parts();
        let mut body = body::aggregate(body).await?;
        let body = body.copy_to_bytes(body.remaining());
        let ClusterState { version } = serde_json::from_slice(&body)?;

        Ok(version)
    }

    async fn get(self, client: HttpClient, path: &str) -> crate::Result<Response<Body>> {
        let mut builder = Request::get(format!("{}{}", self.base_url, path));

        if let Some(authorization) = &self.http_auth {
            builder = authorization.apply_builder(builder);
        }

        for (header, value) in &self.request.headers {
            builder = builder.header(&header[..], &value[..]);
        }

        let mut request = builder.body(Bytes::new())?;

        if let Some(credentials_provider) = &self.aws_auth {
            sign_request(&mut request, credentials_provider, &self.region).await?;
        }
        client
            .send(request.map(hyper::Body::from))
            .await
            .map_err(Into::into)
    }
}

pub async fn sign_request(
    request: &mut http::Request<Bytes>,
    credentials_provider: &SharedCredentialsProvider,
    region: &Option<Region>,
) -> crate::Result<()> {
    crate::aws::sign_request("es", request, credentials_provider, region).await
}

#[derive(Deserialize)]
struct ClusterState {
    version: usize,
}
