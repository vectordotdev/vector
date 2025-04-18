use bytes::{Buf, Bytes};
use http::{Response, StatusCode, Uri};
use hyper::{body, Body};
use serde::Deserialize;
use snafu::ResultExt;
use vector_lib::config::proxy::ProxyConfig;
use vector_lib::config::LogNamespace;

use super::{
    request_builder::ElasticsearchRequestBuilder, ElasticsearchApiVersion, ElasticsearchEncoder,
    InvalidHostSnafu, Request, VersionType,
};
use crate::{
    http::{HttpClient, MaybeAuth, QueryParameterValue, QueryParameters},
    sinks::{
        elasticsearch::{
            ElasticsearchAuthConfig, ElasticsearchCommonMode, ElasticsearchConfig,
            OpenSearchServiceType, ParseError,
        },
        util::{auth::Auth, http::RequestConfig, UriSerde},
        HealthcheckError,
    },
    tls::TlsSettings,
    transforms::metric_to_log::MetricToLog,
};

#[derive(Debug, Clone)]
pub struct ElasticsearchCommon {
    pub base_url: String,
    pub bulk_uri: Uri,
    pub auth: Option<Auth>,
    pub service_type: OpenSearchServiceType,
    pub mode: ElasticsearchCommonMode,
    pub request_builder: ElasticsearchRequestBuilder,
    pub tls_settings: TlsSettings,
    pub request: RequestConfig,
    pub query_params: QueryParameters,
    pub metric_to_log: MetricToLog,
}

impl ElasticsearchCommon {
    pub async fn parse_config(
        config: &ElasticsearchConfig,
        endpoint: &str,
        proxy_config: &ProxyConfig,
        version: &mut Option<usize>,
    ) -> crate::Result<Self> {
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

        let uri = endpoint.parse::<UriSerde>()?;
        let auth = match &config.auth {
            Some(ElasticsearchAuthConfig::Basic { user, password }) => {
                let auth = Some(crate::http::Auth::Basic {
                    user: user.clone(),
                    password: password.clone(),
                });
                // basic auth must be some for now
                let auth = auth.choose_one(&uri.auth)?.unwrap();
                Some(Auth::Basic(auth))
            }
            #[cfg(feature = "aws-core")]
            Some(ElasticsearchAuthConfig::Aws(aws)) => {
                let region = config
                    .aws
                    .as_ref()
                    .map(|config| config.region())
                    .ok_or(ParseError::RegionRequired)?
                    .ok_or(ParseError::RegionRequired)?;
                Some(Auth::Aws {
                    credentials_provider: aws
                        .credentials_provider(region.clone(), proxy_config, config.tls.as_ref())
                        .await?,
                    region,
                })
            }
            None => None,
        };

        if config.opensearch_service_type == OpenSearchServiceType::Serverless {
            match &config.auth {
                #[cfg(feature = "aws-core")]
                Some(ElasticsearchAuthConfig::Aws(_)) => (),
                _ => return Err(ParseError::OpenSearchServerlessRequiresAwsAuth.into()),
            }
        }

        let base_url = uri.uri.to_string().trim_end_matches('/').to_owned();

        let mode = config.common_mode()?;

        let tower_request = config.request.tower.into_settings();

        if config.bulk.version.is_some() && config.bulk.version_type == VersionType::Internal {
            return Err(ParseError::ExternalVersionIgnoredWithInternalVersioning.into());
        }
        if config.bulk.version.is_some()
            && (config.bulk.version_type == VersionType::External
                || config.bulk.version_type == VersionType::ExternalGte)
            && config.id_key.is_none()
        {
            return Err(ParseError::ExternalVersioningWithoutDocumentID.into());
        }
        if config.bulk.version.is_none()
            && (config.bulk.version_type == VersionType::External
                || config.bulk.version_type == VersionType::ExternalGte)
        {
            return Err(ParseError::ExternalVersioningWithoutVersion.into());
        }

        let mut query_params = config.query.clone().unwrap_or_default();
        query_params.insert(
            "timeout".into(),
            QueryParameterValue::SingleParam(format!("{}s", tower_request.timeout.as_secs())),
        );

        if let Some(pipeline) = &config.pipeline {
            if !pipeline.is_empty() {
                query_params.insert(
                    "pipeline".into(),
                    QueryParameterValue::SingleParam(pipeline.into()),
                );
            }
        }

        let bulk_url = {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            // Iterate through the HashMap
            for (param_name, param_value) in &query_params {
                match param_value {
                    QueryParameterValue::SingleParam(param) => {
                        // For single parameter, just append one pair
                        query.append_pair(param_name, param);
                    }
                    QueryParameterValue::MultiParams(params) => {
                        // For multiple parameters, append the same key multiple times
                        for value in params {
                            query.append_pair(param_name, value);
                        }
                    }
                }
            }
            format!("{}/_bulk?{}", base_url, query.finish())
        };
        let bulk_uri = bulk_url.parse::<Uri>().unwrap();

        let tls_settings = TlsSettings::from_options(config.tls.as_ref())?;
        let config = config.clone();
        let request = config.request;

        let metric_config = config.metrics.clone().unwrap_or_default();
        let metric_to_log = MetricToLog::new(
            metric_config.host_tag.as_deref(),
            metric_config.timezone.unwrap_or_default(),
            LogNamespace::Legacy,
            metric_config.metric_tag_values,
        );

        let service_type = config.opensearch_service_type;

        let version = if service_type == OpenSearchServiceType::Serverless {
            if config.api_version != ElasticsearchApiVersion::Auto {
                return Err(ParseError::ServerlessElasticsearchApiVersionMustBeAuto.into());
            }
            // Amazon OpenSearch Serverless does not support the cluster-version API; hardcode
            // well-known API version
            8
        } else if let Some(version) = *version {
            version
        } else {
            let ver = match config.api_version {
                ElasticsearchApiVersion::V6 => 6,
                ElasticsearchApiVersion::V7 => 7,
                ElasticsearchApiVersion::V8 => 8,
                ElasticsearchApiVersion::Auto => {
                    match get_version(
                        &base_url,
                        auth.as_ref(),
                        #[cfg(feature = "aws-core")]
                        &service_type,
                        &request,
                        &tls_settings,
                        proxy_config,
                    )
                    .await
                    {
                        Ok(version) => {
                            debug!(message = "Auto-detected Elasticsearch API version.", %version);
                            version
                        }
                        // This error should be fatal, but for now we only emit it as a warning
                        // to make the transition smoother.
                        Err(error) => {
                            // For now, estimate version.
                            // The `suppress_type_name` option is only valid up to V6, so if a user
                            // specified that is true, then we will assume they need API V6.
                            // Otherwise, assume the latest version (V8).
                            // This is by no means a perfect assumption but it's the best we can
                            // make with the data we have.
                            let assumed_version = if config.suppress_type_name { 6 } else { 8 };
                            debug!(message = "Assumed Elasticsearch API version based on config setting suppress_type_name.",
                                   %assumed_version,
                                   %config.suppress_type_name
                            );
                            warn!(message = "Failed to determine Elasticsearch API version. Please fix the reported error or set an API version explicitly via `api_version`.",
                                  %assumed_version,
                                  %error
                            );
                            assumed_version
                        }
                    }
                }
            };
            *version = Some(ver);
            ver
        };

        let doc_type = config.doc_type.clone();
        let suppress_type_name = if config.suppress_type_name {
            warn!(message = "DEPRECATION, use of deprecated option `suppress_type_name`. Please use `api_version` option instead.");
            config.suppress_type_name
        } else {
            version >= 7
        };
        let request_builder = ElasticsearchRequestBuilder {
            compression: config.compression,
            encoder: ElasticsearchEncoder {
                transformer: config.encoding.clone(),
                doc_type,
                suppress_type_name,
            },
        };

        Ok(Self {
            auth,
            service_type,
            base_url,
            bulk_uri,
            mode,
            request_builder,
            query_params,
            request,
            tls_settings,
            metric_to_log,
        })
    }

    /// Parses endpoints into a vector of ElasticsearchCommons. The resulting vector is guaranteed to not be empty.
    pub async fn parse_many(
        config: &ElasticsearchConfig,
        proxy_config: &ProxyConfig,
    ) -> crate::Result<Vec<Self>> {
        let mut version = None;
        if let Some(endpoint) = config.endpoint.as_ref() {
            warn!(message = "DEPRECATION, use of deprecated option `endpoint`. Please use `endpoints` option instead.");
            if config.endpoints.is_empty() {
                Ok(vec![
                    Self::parse_config(config, endpoint, proxy_config, &mut version).await?,
                ])
            } else {
                Err(ParseError::EndpointsExclusive.into())
            }
        } else if config.endpoints.is_empty() {
            Err(ParseError::EndpointRequired.into())
        } else {
            let mut commons = Vec::new();
            for endpoint in config.endpoints.iter() {
                commons
                    .push(Self::parse_config(config, endpoint, proxy_config, &mut version).await?);
            }
            Ok(commons)
        }
    }

    /// Parses a single endpoint, else panics.
    #[cfg(test)]
    pub async fn parse_single(config: &ElasticsearchConfig) -> crate::Result<Self> {
        let mut commons =
            Self::parse_many(config, crate::config::SinkContext::default().proxy()).await?;
        assert_eq!(commons.len(), 1);
        Ok(commons.remove(0))
    }

    pub async fn healthcheck(self, client: HttpClient) -> crate::Result<()> {
        if self.service_type == OpenSearchServiceType::Serverless {
            warn!(message = "Amazon OpenSearch Serverless does not support healthchecks. Skipping healthcheck...");
            Ok(())
        } else {
            match get(
                &self.base_url,
                self.auth.as_ref(),
                #[cfg(feature = "aws-core")]
                &self.service_type,
                &self.request,
                client,
                "/_cluster/health",
            )
            .await?
            .status()
            {
                StatusCode::OK => Ok(()),
                status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
            }
        }
    }
}

#[cfg(feature = "aws-core")]
pub async fn sign_request(
    service_type: &OpenSearchServiceType,
    request: &mut http::Request<Bytes>,
    credentials_provider: &aws_credential_types::provider::SharedCredentialsProvider,
    region: Option<&aws_types::region::Region>,
) -> crate::Result<()> {
    // Amazon OpenSearch Serverless requires the x-amz-content-sha256 header when calculating
    // the AWS v4 signature:
    // https://docs.aws.amazon.com/opensearch-service/latest/developerguide/serverless-clients.html#serverless-signing
    crate::aws::sign_request(
        service_type.as_str(),
        request,
        credentials_provider,
        region,
        *service_type == OpenSearchServiceType::Serverless,
    )
    .await
}

async fn get_version(
    base_url: &str,
    auth: Option<&Auth>,
    #[cfg(feature = "aws-core")] service_type: &OpenSearchServiceType,
    request: &RequestConfig,
    tls_settings: &TlsSettings,
    proxy_config: &ProxyConfig,
) -> crate::Result<usize> {
    #[derive(Deserialize)]
    struct Version {
        number: Option<String>,
    }
    #[derive(Deserialize)]
    struct ResponsePayload {
        version: Option<Version>,
    }

    let client = HttpClient::new(tls_settings.clone(), proxy_config)?;
    let response = get(
        base_url,
        auth,
        #[cfg(feature = "aws-core")]
        service_type,
        request,
        client,
        "/",
    )
    .await
    .map_err(|error| format!("Failed to get Elasticsearch API version: {}", error))?;

    let (_, body) = response.into_parts();
    let mut body = body::aggregate(body).await?;
    let body = body.copy_to_bytes(body.remaining());
    let ResponsePayload { version } = serde_json::from_slice(&body)?;
    if let Some(version) = version {
        if let Some(number) = version.number {
            let v: Vec<&str> = number.split('.').collect();
            if !v.is_empty() {
                if let Ok(major_version) = v[0].parse::<usize>() {
                    return Ok(major_version);
                }
            }
        }
    }
    Err("Unexpected response from Elasticsearch endpoint `/`. Consider setting `api_version` option.".into())
}

async fn get(
    base_url: &str,
    auth: Option<&Auth>,
    #[cfg(feature = "aws-core")] service_type: &OpenSearchServiceType,
    request: &RequestConfig,
    client: HttpClient,
    path: &str,
) -> crate::Result<Response<Body>> {
    let mut builder = Request::get(format!("{}{}", base_url, path));

    for (header, value) in &request.headers {
        builder = builder.header(&header[..], &value[..]);
    }
    let mut request = builder.body(Bytes::new())?;

    if let Some(auth) = auth {
        match auth {
            Auth::Basic(http_auth) => {
                http_auth.apply(&mut request);
            }
            #[cfg(feature = "aws-core")]
            Auth::Aws {
                credentials_provider: provider,
                region,
            } => {
                let region = region.clone();
                sign_request(service_type, &mut request, provider, Some(&region)).await?;
            }
        }
    }

    client
        .send(request.map(hyper::Body::from))
        .await
        .map_err(Into::into)
}
