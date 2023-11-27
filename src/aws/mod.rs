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
use aws_smithy_runtime::client::http::hyper_014::HyperClientBuilder;
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
use bytes::Bytes;
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
    // type DefaultMiddleware: SmithyMiddleware<DynConnector> + Clone + Send + Sync + 'static;
    // type DefaultMiddleware: SharedInterceptor;

    fn default_middleware() -> Vec<SharedInterceptor>;

    fn build(config: &SdkConfig) -> Self::Client;
}

pub async fn create_smithy_client<T: ClientBuilder>(
    _region: Region,
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
    _is_sink: bool,
    retry_config: RetryConfig,
) -> crate::Result<Config> {
    let tls_settings = MaybeTlsSettings::tls_client(tls_options)?;

    let connector = if proxy.enabled {
        let proxy = build_proxy_connector(tls_settings, proxy)?;
        // let hyper_client = aws_smithy_client::hyper_ext::Adapter::builder().build(proxy);
        HyperClientBuilder::new().build(proxy)
    } else {
        let tls_connector = build_tls_connector(tls_settings)?;
        // let hyper_client = aws_smithy_client::hyper_ext::Adapter::builder().build(tls_connector);
        HyperClientBuilder::new().build(tls_connector)
    };

    // let middleware_builder = ServiceBuilder::new()
    //     .layer(CaptureRequestSize::new(is_sink, region))
    //     .layer(T::default_middleware());
    // let middleware = DynMiddleware::new(middleware_builder);

    let client_builder = Builder::new();
    let mut client_builder = client_builder
        .http_client(connector)
        // .middleware(middleware)
        // .set_interceptors(T::default_middleware());
    // .sleep_impl(Arc::new(TokioSleep::new()));
    client_builder.set_retry_config(Some(retry_config.into()));

    Ok(client_builder.build())
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
    _proxy: &ProxyConfig,
    _tls_options: &Option<TlsConfig>,
    _is_sink: bool,
) -> crate::Result<(T::Client, Region)> {
    let retry_config = RetryConfig::disabled();

    // The default credentials chains will look for a region if not given but we'd like to
    // error up front if later SDK calls will fail due to lack of region configuration
    let region = resolve_region(region).await?;

    let provider_config =
        aws_config::provider_config::ProviderConfig::empty().with_region(Some(region.clone()));

    // Build the configuration first.
    let mut config_builder = SdkConfig::builder()
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

    // let client =
    //     create_smithy_client::<T>(region.clone(), proxy, tls_options, is_sink, retry_config)
    //         .await?;

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

// TODO: Put this back
// Layer for capturing the payload size for AWS API client requests and emitting internal telemetry.
// #[derive(Clone)]
// struct CaptureRequestSize {
//     enabled: bool,
//     region: Region,
// }

// impl Intercept for CaptureRequestSize {
//     fn name(&self) -> &'static str {
//         "CaptureRequestSize"
//     }
// }

// impl CaptureRequestSize {
//     const fn new(enabled: bool, region: Region) -> Self {
//         Self { enabled, region }
//     }
// }

// impl<S> Layer<S> for CaptureRequestSize {
//     type Service = CaptureRequestSizeService<S>;

//     fn layer(&self, inner: S) -> Self::Service {
//         CaptureRequestSizeService {
//             enabled: self.enabled,
//             region: self.region.clone(),
//             inner,
//         }
//     }
// }

// /// Service for capturing the payload size for AWS API client requests and emitting internal telemetry.
// #[derive(Clone)]
// struct CaptureRequestSizeService<S> {
//     enabled: bool,
//     region: Region,
//     inner: S,
// }

// // TODO Does this now need to be an interceptor?
// impl<S> Service<Request> for CaptureRequestSizeService<S>
// where
//     S: Service<Request, Response = Response> + Send + Sync + 'static,
//     S::Future: Send + 'static,
// {
//     type Response = S::Response;
//     type Error = S::Error;
//     type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

//     fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//         self.inner.poll_ready(cx)
//     }

//     fn call(&mut self, req: Request) -> Self::Future {
//         // Attach a body callback that will capture the bytes sent by interrogating the body chunks that get read as it
//         // sends the request out over the wire. We'll read the shared atomic counter, which will contain the number of
//         // bytes "read", aka the bytes it actually sent, if and only if we get back a successful response.
//         let (req, maybe_bytes_sent) = if self.enabled {
//             let shared_bytes_sent = Arc::new(AtomicUsize::new(0));
//             let (request, properties) = req.into_parts();
//             let (parts, body) = request.into_parts();

//             let body = {
//                 let shared_bytes_sent = Arc::clone(&shared_bytes_sent);

//                 body.map_immutable(move |body| {
//                     let body = MeasuredBody::new(body, Arc::clone(&shared_bytes_sent));
//                     SdkBody::from_dyn(BoxBody::new(body))
//                 })
//             };

//             let req = Request::from_parts(http::Request::from_parts(parts, body), properties);

//             (req, Some(shared_bytes_sent))
//         } else {
//             (req, None)
//         };

//         let region = self.region.clone();
//         let fut = self.inner.call(req);

//         Box::pin(async move {
//             // Perform the actual API call and see if it was successful by HTTP status code standards. If so, we emit a
//             // `BytesSent` event to ensure that we capture the data flowing out as API calls.
//             let result = fut.await;
//             if let Ok(response) = &result {
//                 let byte_size = maybe_bytes_sent
//                     .map(|s| s.load(Ordering::Acquire))
//                     .unwrap_or(0);

//                 // TODO: Should we actually emit for any other range of status codes? Right now, `is_success` is true
//                 // for `200 <= status < 300`, which feels comprehensive... but are there other valid statuses?
//                 if response.http().status().is_success() && byte_size != 0 {
//                     emit!(AwsBytesSent {
//                         byte_size,
//                         region: Some(region),
//                     });
//                 }
//             }

//             result
//         })
//     }
// }

// #[pin_project]
// struct MeasuredBody {
//     #[pin]
//     inner: SdkBody,
//     shared_bytes_sent: Arc<AtomicUsize>,
// }

// impl MeasuredBody {
//     fn new(body: SdkBody, shared_bytes_sent: Arc<AtomicUsize>) -> Self {
//         Self {
//             inner: body,
//             shared_bytes_sent,
//         }
//     }
// }

// impl Body for MeasuredBody {
//     type Data = Bytes;
//     type Error = Box<dyn Error + Send + Sync>;

//     fn poll_data(
//         self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//     ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
//         let this = self.project();

//         match this.inner.poll_data(cx) {
//             Poll::Ready(Some(Ok(data))) => {
//                 this.shared_bytes_sent
//                     .fetch_add(data.len(), Ordering::Release);
//                 Poll::Ready(Some(Ok(data)))
//             }
//             Poll::Ready(None) => Poll::Ready(None),
//             Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
//             Poll::Pending => Poll::Pending,
//         }
//     }

//     fn poll_trailers(
//         self: Pin<&mut Self>,
//         _cx: &mut Context<'_>,
//     ) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
//         Poll::Ready(Ok(None))
//     }
// }
