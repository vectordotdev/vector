pub mod auth;
pub mod region;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};

pub use auth::{AwsAuthentication, ImdsAuthentication};
use aws_config::meta::region::ProvideRegion;
use aws_sigv4::http_request::{SignableRequest, SigningSettings};
use aws_sigv4::SigningParams;
use aws_smithy_async::rt::sleep::{AsyncSleep, Sleep};
use aws_smithy_client::bounds::SmithyMiddleware;
use aws_smithy_client::erase::{DynConnector, DynMiddleware};
use aws_smithy_client::{Builder, SdkError};
use aws_smithy_http::callback::BodyCallback;
use aws_smithy_http::endpoint::Endpoint;
use aws_smithy_http::event_stream::BoxError;
use aws_smithy_http::operation::{Request, Response};
use aws_smithy_types::retry::RetryConfig;
use aws_types::credentials::{ProvideCredentials, SharedCredentialsProvider};
use aws_types::region::Region;
use aws_types::SdkConfig;
use bytes::Bytes;
use once_cell::sync::OnceCell;
use regex::RegexSet;
pub use region::RegionOrEndpoint;
use tower::{Layer, Service, ServiceBuilder};

use crate::config::ProxyConfig;
use crate::http::{build_proxy_connector, build_tls_connector};
use crate::internal_events::AwsBytesSent;
use crate::tls::{MaybeTlsSettings, TlsConfig};

static RETRIABLE_CODES: OnceCell<RegexSet> = OnceCell::new();

pub fn is_retriable_error<T>(error: &SdkError<T>) -> bool {
    match error {
        SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => true,
        SdkError::ConstructionFailure(_) => false,
        SdkError::ResponseError { err: _, raw } | SdkError::ServiceError { err: _, raw } => {
            // This header is a direct indication that we should retry the request. Eventually it'd
            // be nice to actually schedule the retry after the given delay, but for now we just
            // check that it contains a positive value.
            let retry_header = raw.http().headers().get("x-amz-retry-after").is_some();

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

            let status = raw.http().status();
            let response_body = String::from_utf8_lossy(raw.http().body().bytes().unwrap_or(&[]));

            retry_header
                || status.is_server_error()
                || status == http::StatusCode::TOO_MANY_REQUESTS
                || (status.is_client_error() && re.is_match(response_body.as_ref()))
        }
    }
}

pub trait ClientBuilder {
    type Config;
    type Client;
    type DefaultMiddleware: SmithyMiddleware<DynConnector> + Clone + Send + Sync + 'static;

    fn default_middleware() -> Self::DefaultMiddleware;

    fn build(client: aws_smithy_client::Client, config: &aws_types::SdkConfig) -> Self::Client;
}

pub async fn create_smithy_client<T: ClientBuilder>(
    region: Region,
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
    is_sink: bool,
    retry_config: RetryConfig,
) -> crate::Result<aws_smithy_client::Client> {
    let tls_settings = MaybeTlsSettings::tls_client(tls_options)?;

    let connector = if proxy.enabled {
        let proxy = build_proxy_connector(tls_settings, proxy)?;
        let hyper_client = aws_smithy_client::hyper_ext::Adapter::builder().build(proxy);
        aws_smithy_client::erase::DynConnector::new(hyper_client)
    } else {
        let tls_connector = build_tls_connector(tls_settings)?;
        let hyper_client = aws_smithy_client::hyper_ext::Adapter::builder().build(tls_connector);
        aws_smithy_client::erase::DynConnector::new(hyper_client)
    };

    let middleware_builder = ServiceBuilder::new()
        .layer(CaptureRequestSize::new(is_sink, region))
        .layer(T::default_middleware());
    let middleware = DynMiddleware::new(middleware_builder);

    let mut client_builder = Builder::new()
        .connector(connector)
        .middleware(middleware)
        .sleep_impl(Arc::new(TokioSleep));
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
    endpoint: Option<Endpoint>,
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
    is_sink: bool,
) -> crate::Result<T::Client> {
    let retry_config = RetryConfig::disabled();

    // The default credentials chains will look for a region if not given but we'd like to
    // error up front if later SDK calls will fail due to lack of region configuration
    let region = resolve_region(region).await?;

    // Build the configuration first.
    let mut config_builder = SdkConfig::builder()
        .credentials_provider(auth.credentials_provider(region.clone()).await?)
        .region(region.clone())
        .retry_config(retry_config.clone());

    if let Some(endpoint_override) = endpoint {
        config_builder = config_builder.endpoint_resolver(endpoint_override);
    }

    let config = config_builder.build();

    let client =
        create_smithy_client::<T>(region, proxy, tls_options, is_sink, retry_config).await?;

    Ok(T::build(client, &config))
}

pub async fn sign_request(
    service_name: &str,
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
        .service_name(service_name)
        .time(SystemTime::now())
        .settings(SigningSettings::default());

    signing_params_builder.set_security_token(credentials.session_token());

    let (signing_instructions, _signature) =
        aws_sigv4::http_request::sign(signable_request, &signing_params_builder.build()?)?
            .into_parts();
    signing_instructions.apply_to_request(request);

    Ok(())
}

#[derive(Debug)]
pub struct TokioSleep;

impl AsyncSleep for TokioSleep {
    fn sleep(&self, duration: Duration) -> Sleep {
        Sleep::new(tokio::time::sleep(duration))
    }
}

/// Layer for capturing the payload size for AWS API client requests and emitting internal telemetry.
#[derive(Clone)]
struct CaptureRequestSize {
    enabled: bool,
    region: Region,
}

impl CaptureRequestSize {
    const fn new(enabled: bool, region: Region) -> Self {
        Self { enabled, region }
    }
}

impl<S> Layer<S> for CaptureRequestSize {
    type Service = CaptureRequestSizeService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CaptureRequestSizeService {
            enabled: self.enabled,
            region: self.region.clone(),
            inner,
        }
    }
}

/// Service for capturing the payload size for AWS API client requests and emitting internal telemetry.
#[derive(Clone)]
struct CaptureRequestSizeService<S> {
    enabled: bool,
    region: Region,
    inner: S,
}

impl<S> Service<Request> for CaptureRequestSizeService<S>
where
    S: Service<Request, Response = Response> + Send + Sync + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        // Attach a body callback that will capture the bytes sent by interrogating the body chunks that get read as it
        // sends the request out over the wire. We'll read the shared atomic counter, which will contain the number of
        // bytes "read", aka the bytes it actually sent, if and only if we get back a successful response.
        let maybe_bytes_sent = self.enabled.then(|| {
            let (callback, shared_bytes_sent) = BodyCaptureCallback::new();
            req.http_mut().body_mut().with_callback(Box::new(callback));

            shared_bytes_sent
        });

        let region = self.region.clone();
        let fut = self.inner.call(req);

        Box::pin(async move {
            // Perform the actual API call and see if it was successful by HTTP status code standards. If so, we emit a
            // `BytesSent` event to ensure that we capture the data flowing out as API calls.
            let result = fut.await;
            if let Ok(response) = &result {
                let byte_size = maybe_bytes_sent
                    .map(|s| s.load(Ordering::Acquire))
                    .unwrap_or(0);

                // TODO: Should we actually emit for any other range of status codes? Right now, `is_success` is true
                // for `200 <= status < 300`, which feels comprehensive... but are there other valid statuses?
                if response.http().status().is_success() && byte_size != 0 {
                    emit!(AwsBytesSent {
                        byte_size,
                        region: Some(region),
                    });
                }
            }

            result
        })
    }
}

struct BodyCaptureCallback {
    bytes_sent: usize,
    shared_bytes_sent: Arc<AtomicUsize>,
}

impl BodyCaptureCallback {
    fn new() -> (Self, Arc<AtomicUsize>) {
        let shared_bytes_sent = Arc::new(AtomicUsize::new(0));

        (
            Self {
                bytes_sent: 0,
                shared_bytes_sent: Arc::clone(&shared_bytes_sent),
            },
            shared_bytes_sent,
        )
    }
}

impl BodyCallback for BodyCaptureCallback {
    fn update(&mut self, bytes: &[u8]) -> Result<(), BoxError> {
        // This gets called every time a chunk is read from the request body, which includes both static chunks and
        // streaming bodies. Just add the chunk's length to our running tally.
        self.bytes_sent += bytes.len();
        Ok(())
    }

    fn trailers(&self) -> Result<Option<headers::HeaderMap<headers::HeaderValue>>, BoxError> {
        Ok(None)
    }

    fn make_new(&self) -> Box<dyn BodyCallback> {
        // We technically don't use retries within the AWS side of the API clients, but we have to satisfy this trait
        // method, because `aws_smithy_http` uses the retry layer from `tower`, which clones the request regardless
        // before it even executes the first attempt... so there's no reason not to make it technically correct.
        Box::new(Self {
            bytes_sent: 0,
            shared_bytes_sent: Arc::clone(&self.shared_bytes_sent),
        })
    }
}

impl Drop for BodyCaptureCallback {
    fn drop(&mut self) {
        // This is where we actually emit. We specifically emit here, and not in `trailers`, because despite the
        // documentation that `trailers` is called after all chunks of the body are successfully read, `hyper` won't
        // continue polling a body if it knows it's gotten all the available bytes i.e. it doesn't necessarily drive it
        // until `poll_data` returns `None`. This means the only consistent place to know that the body is "done" is
        // when it's dropped.
        //
        // We update our shared atomic counter with the total bytes sent that we accumulated, and it will read the
        // atomic if the response indicates that the request was successful. Since we know the body will go out-of-scope
        // before a response can possibly be generated, we know the atomic will in turn be updated before it is read.
        //
        // This design also copes with the fact that, technically, `aws_smithy_client` supports retries and could clone
        // this callback for each copy of the request... which it already does at least once per request since the retry
        // middleware has to clone the request before trying it. As requests are retried sequentially, only after the
        // previous attempt failed, we know that we'll end up in a "last write wins" scenario, so this is still sound.
        //
        // In the future, we may track every single byte sent in order to generate "raw bytes over the wire, regardless
        // of status" metrics, but right now, this is purely "how many bytes have we sent as part of _successful_
        // sends?"
        self.shared_bytes_sent
            .store(self.bytes_sent, Ordering::Release);
    }
}
