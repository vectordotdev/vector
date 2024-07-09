//! Shared functionality for the AWS components.
pub mod auth;
pub mod region;
pub mod timeout;

pub use auth::{AwsAuthentication, ImdsAuthentication};
use aws_config::{
    meta::region::ProvideRegion, retry::RetryConfig, timeout::TimeoutConfig, Region, SdkConfig,
};
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sigv4::{
    http_request::{SignableBody, SignableRequest, SigningSettings},
    sign::v4,
};
use aws_smithy_async::rt::sleep::TokioSleep;
use aws_smithy_runtime::client::http::hyper_014::HyperClientBuilder;
use aws_smithy_runtime_api::client::{
    http::{
        HttpClient, HttpConnector, HttpConnectorFuture, HttpConnectorSettings, SharedHttpConnector,
    },
    identity::Identity,
    orchestrator::{HttpRequest, HttpResponse},
    result::SdkError,
    runtime_components::RuntimeComponents,
};
use aws_smithy_types::body::SdkBody;
use aws_types::sdk_config::SharedHttpClient;
use bytes::Bytes;
use futures_util::FutureExt;
use http::HeaderMap;
use http_body::{combinators::BoxBody, Body};
use pin_project::pin_project;
use regex::RegexSet;
pub use region::RegionOrEndpoint;
use snafu::Snafu;
use std::{
    error::Error,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
    task::{Context, Poll},
    time::{Duration, SystemTime},
};
pub use timeout::AwsTimeout;

use crate::config::ProxyConfig;
use crate::http::{build_proxy_connector, build_tls_connector, status};
use crate::internal_events::AwsBytesSent;
use crate::tls::{MaybeTlsSettings, TlsConfig};

static RETRIABLE_CODES: OnceLock<RegexSet> = OnceLock::new();

/// Checks if the request can be retried after the given error was returned.
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
        || status.as_u16() == status::TOO_MANY_REQUESTS
        || (status.is_client_error() && re.is_match(response_body.as_ref()))
}

/// Creates the http connector that has been configured to use the given proxy and TLS settings.
/// All AWS requests should use this connector as the aws crates by default use RustTLS which we
/// have turned off as we want to consistently use openssl.
fn connector(
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
) -> crate::Result<SharedHttpClient> {
    let tls_settings = MaybeTlsSettings::tls_client(tls_options)?;

    if proxy.enabled {
        let proxy = build_proxy_connector(tls_settings, proxy)?;
        Ok(HyperClientBuilder::new().build(proxy))
    } else {
        let tls_connector = build_tls_connector(tls_settings)?;
        Ok(HyperClientBuilder::new().build(tls_connector))
    }
}

/// Implement for each AWS service to create the appropriate AWS sdk client.
pub trait ClientBuilder {
    /// The type of the client in the SDK.
    type Client;

    /// Build the client using the given config settings.
    fn build(config: &SdkConfig) -> Self::Client;
}

fn region_provider(
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
) -> crate::Result<impl ProvideRegion> {
    let config = aws_config::provider_config::ProviderConfig::default()
        .with_http_client(connector(proxy, tls_options)?);

    Ok(aws_config::meta::region::RegionProviderChain::first_try(
        aws_config::environment::EnvironmentVariableRegionProvider::new(),
    )
    .or_else(aws_config::profile::ProfileFileRegionProvider::builder().build())
    .or_else(
        aws_config::imds::region::ImdsRegionProvider::builder()
            .configure(&config)
            .build(),
    ))
}

async fn resolve_region(
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
    region: Option<Region>,
) -> crate::Result<Region> {
    match region {
        Some(region) => Ok(region),
        None => region_provider(proxy, tls_options)?
            .region()
            .await
            .ok_or_else(|| {
                "Could not determine region from Vector configuration or default providers".into()
            }),
    }
}

/// Create the SDK client using the provided settings.
pub async fn create_client<T: ClientBuilder>(
    auth: &AwsAuthentication,
    region: Option<Region>,
    endpoint: Option<String>,
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
    timeout: &Option<AwsTimeout>,
) -> crate::Result<T::Client> {
    create_client_and_region::<T>(auth, region, endpoint, proxy, tls_options, timeout)
        .await
        .map(|(client, _)| client)
}

/// Create the SDK client and resolve the region using the provided settings.
pub async fn create_client_and_region<T: ClientBuilder>(
    auth: &AwsAuthentication,
    region: Option<Region>,
    endpoint: Option<String>,
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
    timeout: &Option<AwsTimeout>,
) -> crate::Result<(T::Client, Region)> {
    let retry_config = RetryConfig::disabled();

    // The default credentials chains will look for a region if not given but we'd like to
    // error up front if later SDK calls will fail due to lack of region configuration
    let region = resolve_region(proxy, tls_options, region).await?;

    let provider_config =
        aws_config::provider_config::ProviderConfig::empty().with_region(Some(region.clone()));

    let connector = connector(proxy, tls_options)?;

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
        .credentials_provider(
            auth.credentials_provider(region.clone(), proxy, tls_options)
                .await?,
        )
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

    if let Some(timeout) = timeout {
        let mut timeout_config_builder = TimeoutConfig::builder();

        let operation_timeout = timeout.operation_timeout();
        let connect_timeout = timeout.connect_timeout();
        let read_timeout = timeout.read_timeout();

        timeout_config_builder
            .set_operation_timeout(operation_timeout.map(Duration::from_secs))
            .set_connect_timeout(connect_timeout.map(Duration::from_secs))
            .set_read_timeout(read_timeout.map(Duration::from_secs));

        config_builder = config_builder.timeout_config(timeout_config_builder.build());
    }

    let config = config_builder.build();

    Ok((T::build(&config), region))
}

#[derive(Snafu, Debug)]
enum SigningError {
    #[snafu(display("cannot sign the request because the headers are not valid utf-8"))]
    NotUTF8Header,
}

/// Sign the request prior to sending to AWS.
/// The signature is added to the provided `request`.
pub async fn sign_request(
    service_name: &str,
    request: &mut http::Request<Bytes>,
    credentials_provider: &SharedCredentialsProvider,
    region: &Option<Region>,
) -> crate::Result<()> {
    let headers = request
        .headers()
        .iter()
        .map(|(k, v)| {
            Ok((
                k.as_str(),
                std::str::from_utf8(v.as_bytes()).map_err(|_| SigningError::NotUTF8Header)?,
            ))
        })
        .collect::<Result<Vec<_>, SigningError>>()?;

    let signable_request = SignableRequest::new(
        request.method().as_str(),
        request.uri().to_string(),
        headers.into_iter(),
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

    let signing_params = signing_params_builder
        .build()
        .expect("all signing params set");

    let (signing_instructions, _signature) =
        aws_sigv4::http_request::sign(signable_request, &signing_params.into())?.into_parts();
    signing_instructions.apply_to_request_http0x(request);

    Ok(())
}

#[derive(Debug)]
struct AwsHttpClient<T> {
    http: T,
    region: Region,
}

impl<T> HttpClient for AwsHttpClient<T>
where
    T: HttpClient,
{
    fn http_connector(
        &self,
        settings: &HttpConnectorSettings,
        components: &RuntimeComponents,
    ) -> SharedHttpConnector {
        let http_connector = self.http.http_connector(settings, components);

        SharedHttpConnector::new(AwsConnector {
            region: self.region.clone(),
            http: http_connector,
        })
    }
}

#[derive(Clone, Debug)]
struct AwsConnector<T> {
    http: T,
    region: Region,
}

impl<T> HttpConnector for AwsConnector<T>
where
    T: HttpConnector,
{
    fn call(&self, req: HttpRequest) -> HttpConnectorFuture {
        let bytes_sent = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let req = req.map(|body| {
            let bytes_sent = Arc::clone(&bytes_sent);
            body.map_preserve_contents(move |body| {
                let body = MeasuredBody::new(body, Arc::clone(&bytes_sent));
                SdkBody::from_body_0_4(BoxBody::new(body))
            })
        });

        let fut = self.http.call(req);
        let region = self.region.clone();

        HttpConnectorFuture::new(fut.inspect(move |result| {
            let byte_size = bytes_sent.load(Ordering::Relaxed);
            if let Ok(result) = result {
                if result.status().is_success() {
                    emit!(AwsBytesSent {
                        byte_size,
                        region: Some(region),
                    });
                }
            }
        }))
    }
}

#[pin_project]
struct MeasuredBody {
    #[pin]
    inner: SdkBody,
    shared_bytes_sent: Arc<AtomicUsize>,
}

impl MeasuredBody {
    fn new(body: SdkBody, shared_bytes_sent: Arc<AtomicUsize>) -> Self {
        Self {
            inner: body,
            shared_bytes_sent,
        }
    }
}

impl Body for MeasuredBody {
    type Data = Bytes;
    type Error = Box<dyn Error + Send + Sync>;

    fn poll_data(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let this = self.project();

        match this.inner.poll_data(cx) {
            Poll::Ready(Some(Ok(data))) => {
                this.shared_bytes_sent
                    .fetch_add(data.len(), Ordering::Release);
                Poll::Ready(Some(Ok(data)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }
}
