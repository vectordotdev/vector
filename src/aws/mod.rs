#![allow(missing_docs)]
#![allow(unused_imports)]
pub mod auth;
pub mod region;

use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::SystemTime;

pub use auth::{AwsAuthentication, ImdsAuthentication};
use aws_config::{meta::region::ProvideRegion, retry::RetryConfig, Region, SdkConfig};
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sdk_sqs::{config::Builder, Config};
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningParams, SigningSettings};
use aws_sigv4::sign::v4;
use aws_smithy_async::rt::sleep::TokioSleep;
use aws_smithy_runtime::client::http::hyper_014::HyperClientBuilder;
use aws_smithy_runtime_api::client::http::{HttpConnector, SharedHttpConnector};
use aws_smithy_runtime_api::client::identity::Identity;
use aws_smithy_runtime_api::client::orchestrator::HttpRequest;
use aws_smithy_runtime_api::{
    client::{
        interceptors::{Intercept, SharedInterceptor},
        orchestrator::HttpResponse,
        result::SdkError,
    },
    http::{Request, Response, StatusCode},
};
use aws_smithy_types::body::SdkBody;
use aws_types::sdk_config::SharedHttpClient;
use bytes::Bytes;
use futures_util::FutureExt;
use http::HeaderMap;
use http_body::Body;
use pin_project::pin_project;
use regex::RegexSet;
pub use region::RegionOrEndpoint;
use tower::{Layer, Service};

use crate::config::ProxyConfig;
use crate::http::{build_proxy_connector, build_tls_connector};
use crate::internal_events::AwsBytesSent;
use crate::tls::{MaybeTlsSettings, TlsConfig};

static RETRIABLE_CODES: OnceLock<RegexSet> = OnceLock::new();

pub fn is_retriable_error<T>(error: &SdkError<T, HttpResponse>) -> bool {
    match error {
        SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => true,
        SdkError::ConstructionFailure(_) => false,
        SdkError::ResponseError(err) => check_response(err.raw()),
        SdkError::ServiceError(err) => check_response(err.raw()),
        _ => {
            warn!("AWS returned unknown error, retrying request.");
            true
        }
    }
}

fn check_response(res: &HttpResponse) -> bool {
    // This header is a direct indication that we should retry the request. Eventually it'd
    // be nice to actually schedule the retry after the given delay, but for now we just
    // check that it contains a positive value.
    let retry_header = res.headers().get("x-amz-retry-after").is_some();

    // Certain 400-level responses will contain an error code indicating that the request
    // should be retried. Since we don't retry 400-level responses by default, we'll look
    // for these specifically before falling back to more general heuristics. Because AWS
    // services use a mix of XML and JSON response bodies and the AWS SDK doesn't give us
    // a parsed representation, we resort to a simple string match.
    //
    // S3: RequestTimeout
    // SQS: RequestExpired, ThrottlingException
    // ECS: RequestExpired, ThrottlingException
    // Kinesis: RequestExpired, ThrottlingException
    // Cloudwatch: RequestExpired, ThrottlingException
    //
    // Now just look for those when it's a client_error
    let re = RETRIABLE_CODES.get_or_init(|| {
        RegexSet::new(["RequestTimeout", "RequestExpired", "ThrottlingException"])
            .expect("invalid regex")
    });

    let status = res.status();
    let response_body = String::from_utf8_lossy(res.body().bytes().unwrap_or(&[]));

    retry_header
        || status.is_server_error()
        || status.as_u16() == 429 //StatusCode::TOO_MANY_REQUESTS <- TODO We should really have these as constants.
        || (status.is_client_error() && re.is_match(response_body.as_ref()))
}

pub trait ClientBuilder {
    type Config;
    type Client;

    fn default_middleware() -> Vec<SharedInterceptor>;

    fn build(config: &SdkConfig) -> Self::Client;
}

pub async fn resolve_region(region: Option<Region>) -> crate::Result<Region> {
    match region {
        Some(region) => Ok(region),
        None => aws_config::default_provider::region::default_provider()
            .region()
            .await
            .ok_or_else(|| {
                "Could not determine region from Vector configuration or default providers".into()
            }),
    }
}

pub async fn create_client<T: ClientBuilder>(
    auth: &AwsAuthentication,
    region: Option<Region>,
    endpoint: Option<String>,
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
    is_sink: bool,
) -> crate::Result<T::Client> {
    create_client_and_region::<T>(auth, region, endpoint, proxy, tls_options, is_sink)
        .await
        .map(|(client, _)| client)
}

pub async fn create_client_and_region<T: ClientBuilder>(
    auth: &AwsAuthentication,
    region: Option<Region>,
    endpoint: Option<String>,
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
    _is_sink: bool,
) -> crate::Result<(T::Client, Region)> {
    let retry_config = RetryConfig::disabled();

    // The default credentials chains will look for a region if not given but we'd like to
    // error up front if later SDK calls will fail due to lack of region configuration
    let region = resolve_region(region).await?;

    let provider_config =
        aws_config::provider_config::ProviderConfig::empty().with_region(Some(region.clone()));

    let tls_settings = MaybeTlsSettings::tls_client(tls_options)?;

    let connector = if proxy.enabled {
        let proxy = build_proxy_connector(tls_settings, proxy)?;
        HyperClientBuilder::new().build(proxy)
    } else {
        let tls_connector = build_tls_connector(tls_settings)?;
        HyperClientBuilder::new().build(tls_connector)
    };

    // Create a custom http connector that will emit the required metrics for us.
    let connector = AwsHttpClient {
        http: connector,
        region: region.clone(),
    };

    // Build the configuration first.
    let mut config_builder = SdkConfig::builder()
        .http_client(connector)
        .sleep_impl(Arc::new(TokioSleep::new()))
        .identity_cache(auth.credentials_cache().await?)
        .credentials_provider(auth.credentials_provider(region.clone()).await?)
        .region(region.clone())
        .retry_config(retry_config.clone());

    if let Some(endpoint_override) = endpoint {
        config_builder = config_builder.endpoint_url(endpoint_override);
    }

    if let Some(use_fips) =
        aws_config::default_provider::use_fips::use_fips_provider(&provider_config).await
    {
        config_builder = config_builder.use_fips(use_fips);
    }

    let config = config_builder.build();

    Ok((T::build(&config), region))
}

pub async fn sign_request(
    service_name: &str,
    request: &mut http::Request<Bytes>,
    credentials_provider: &SharedCredentialsProvider,
    region: &Option<Region>,
) -> crate::Result<()> {
    let signable_request = SignableRequest::new(
        request.method().as_str(),
        request.uri().to_string(),
        request.headers().iter().map(|(k, v)| {
            (
                k.as_str(),
                // TODO No unwrap()
                std::str::from_utf8(v.as_bytes()).expect("only utf8 headers are signable"),
            )
        }),
        SignableBody::Bytes(request.body().as_ref()),
    )?;

    let credentials = credentials_provider.provide_credentials().await?;
    let identity = Identity::new(credentials, None);
    let signing_params_builder = v4::SigningParams::builder()
        .identity(&identity)
        .region(region.as_ref().map(|r| r.as_ref()).unwrap_or(""))
        .name(service_name)
        .time(SystemTime::now())
        .settings(SigningSettings::default());

    // TODO SMW - wut is this?
    // signing_params_builder.set_security_token(credentials.session_token());

    let signing_params = signing_params_builder
        .build()
        .expect("all signing params set");

    let (signing_instructions, _signature) =
        aws_sigv4::http_request::sign(signable_request, &signing_params.into())?.into_parts();
    signing_instructions.apply_to_request_http0x(request);

    Ok(())
}

#[derive(Debug)]
pub struct AwsHttpClient<T> {
    http: T,
    region: Region,
}

impl<T> aws_smithy_runtime_api::client::http::HttpClient for AwsHttpClient<T>
where
    T: aws_smithy_runtime_api::client::http::HttpClient,
{
    fn http_connector(
        &self,
        settings: &aws_smithy_runtime_api::client::http::HttpConnectorSettings,
        components: &aws_sdk_cloudwatch::config::RuntimeComponents,
    ) -> SharedHttpConnector {
        let http_connector = self.http.http_connector(settings, components);

        SharedHttpConnector::new(AwsConnector {
            region: self.region.clone(),
            http: http_connector,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AwsConnector<T> {
    http: T,
    region: Region,
}

impl<T> aws_smithy_runtime_api::client::http::HttpConnector for AwsConnector<T>
where
    T: aws_smithy_runtime_api::client::http::HttpConnector,
{
    fn call(&self, req: HttpRequest) -> aws_smithy_runtime_api::client::http::HttpConnectorFuture {
        let byte_size = req.body().bytes().expect("body cannot be lazy").len();
        let fut = self.http.call(req);
        let region = self.region.clone();

        aws_smithy_runtime_api::client::http::HttpConnectorFuture::new(fut.inspect(move |result| {
            if let Ok(result) = result {
                if result.status().is_success() {
                    emit!(AwsBytesSent {
                        byte_size,
                        region: Some(region.clone()),
                    });
                }
            }
        }))
    }
}
