#![cfg(feature = "rusoto_core")]

use crate::{dns::Resolver, sinks::util, tls::MaybeTlsSettings};
use futures::{compat::Compat, future::BoxFuture, TryFutureExt, TryStreamExt};
use futures01::{
    future::{Future as Future01, FutureResult},
    Async, Poll as Poll01, Stream,
};
use http02::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method, Request, Response,
};
use http_body::Body;
use rusoto_core::{
    request::{DispatchSignedRequest, HttpDispatchError, HttpResponse},
    signature::{SignedRequest, SignedRequestPayload},
    ByteStream, CredentialsError, Region,
};
use rusoto_credential::{
    AutoRefreshingProvider, AutoRefreshingProviderFuture, AwsCredentials, ChainProvider,
    ProvideAwsCredentials, StaticProvider,
};
use rusoto_sts::{StsAssumeRoleSessionCredentialsProvider, StsClient};
use snafu::{ResultExt, Snafu};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use task_compat::with_notify;
use tower03::{Service, ServiceExt};

pub type Client = HttpClient<util::http2::HttpClient<RusotoBody>>;

pub fn client(resolver: Resolver) -> crate::Result<Client> {
    let settings = MaybeTlsSettings::enable_client()?;
    let client = util::http2::HttpClient::new(resolver, settings)?;
    Ok(HttpClient { client })
}

#[derive(Debug, Snafu)]
enum RusotoError {
    #[snafu(display("Invalid AWS credentials: {}", source))]
    InvalidAWSCredentials { source: CredentialsError },
}

// A place-holder for the types of AWS credentials we support
pub enum AwsCredentialsProvider {
    Default(AutoRefreshingProvider<ChainProvider>),
    Role(AutoRefreshingProvider<StsAssumeRoleSessionCredentialsProvider>),
    Static(StaticProvider),
}

impl AwsCredentialsProvider {
    pub fn new(region: &Region, assume_role: Option<String>) -> crate::Result<Self> {
        if let Some(role) = assume_role {
            debug!("using sts assume role credentials for AWS.");
            let sts = StsClient::new(region.clone());

            let provider = StsAssumeRoleSessionCredentialsProvider::new(
                sts,
                role,
                "default".to_owned(),
                None,
                None,
                None,
                None,
            );

            let creds = AutoRefreshingProvider::new(provider).context(InvalidAWSCredentials)?;
            Ok(Self::Role(creds))
        } else {
            debug!("using default credentials provider for AWS.");
            let mut chain = ChainProvider::new();
            // 8 seconds because our default healthcheck timeout
            // is 10 seconds.
            chain.set_timeout(Duration::from_secs(8));

            let creds = AutoRefreshingProvider::new(chain).context(InvalidAWSCredentials)?;

            Ok(Self::Default(creds))
        }
    }

    pub fn new_minimal<A: Into<String>, S: Into<String>>(access_key: A, secret_key: S) -> Self {
        Self::Static(StaticProvider::new_minimal(
            access_key.into(),
            secret_key.into(),
        ))
    }
}

impl ProvideAwsCredentials for AwsCredentialsProvider {
    type Future = AwsCredentialsProviderFuture;

    fn credentials(&self) -> Self::Future {
        match self {
            Self::Default(p) => AwsCredentialsProviderFuture::Default(p.credentials()),
            Self::Role(p) => AwsCredentialsProviderFuture::Role(p.credentials()),
            Self::Static(p) => AwsCredentialsProviderFuture::Static(p.credentials()),
        }
    }
}

pub enum AwsCredentialsProviderFuture {
    Default(AutoRefreshingProviderFuture<ChainProvider>),
    Role(AutoRefreshingProviderFuture<StsAssumeRoleSessionCredentialsProvider>),
    Static(FutureResult<AwsCredentials, CredentialsError>),
}

impl Future01 for AwsCredentialsProviderFuture {
    type Item = AwsCredentials;
    type Error = CredentialsError;

    fn poll(&mut self) -> Poll01<Self::Item, Self::Error> {
        match self {
            Self::Default(f) => f.poll(),
            Self::Role(f) => f.poll(),
            Self::Static(f) => f.poll(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpClient<T> {
    client: T,
}

#[derive(Debug)]
pub struct RusotoBody {
    inner: Option<SignedRequestPayload>,
}

impl<T> HttpClient<T> {
    pub fn new(client: T) -> Self {
        HttpClient { client }
    }
}

impl<T> DispatchSignedRequest for HttpClient<T>
where
    T: Service<Request<RusotoBody>, Response = Response<hyper13::Body>, Error = hyper13::Error>
        + Clone
        + Send
        + 'static,
    T::Future: Send + 'static,
{
    type Future = Compat<BoxFuture<'static, Result<HttpResponse, HttpDispatchError>>>;

    // Adaptation of https://docs.rs/rusoto_core/0.41.0/src/rusoto_core/request.rs.html#409-522
    fn dispatch(&self, request: SignedRequest, timeout: Option<Duration>) -> Self::Future {
        assert!(timeout.is_none(), "timeout is not supported at this level");

        let client = self.client.clone();

        let fut = Box::pin(async move {
            let method = match request.method().as_ref() {
                "POST" => Method::POST,
                "PUT" => Method::PUT,
                "DELETE" => Method::DELETE,
                "GET" => Method::GET,
                "HEAD" => Method::HEAD,
                v => unimplemented!("method type: {:?}", v),
            };

            let mut headers = HeaderMap::new();
            for h in request.headers().iter() {
                let header_name = match h.0.parse::<HeaderName>() {
                    Ok(name) => name,
                    Err(err) => {
                        return Err(HttpDispatchError::new(format!("ParseHeader: {}", err)))
                    }
                };
                for v in h.1.iter() {
                    let header_value = match HeaderValue::from_bytes(v) {
                        Ok(value) => value,
                        Err(err) => {
                            return Err(HttpDispatchError::new(format!(
                                "HeaderValueParse: {}",
                                err
                            )))
                        }
                    };
                    headers.append(&header_name, header_value);
                }
            }

            let mut uri = format!(
                "{}://{}{}",
                request.scheme(),
                request.hostname(),
                request.canonical_path()
            );

            if !request.canonical_query_string().is_empty() {
                uri += &format!("?{}", request.canonical_query_string());
            }

            let mut request = Request::builder()
                .method(method)
                .uri(uri)
                .body(RusotoBody::from(request.payload))
                .map_err(|e| format!("RequestBuildingError: {}", e))
                .map_err(HttpDispatchError::new)?;

            *request.headers_mut() = headers;

            let response = client
                .oneshot(request)
                .await
                .map_err(|e| HttpDispatchError::new(format!("DispatchError: {}", e)))?;

            let status = http::StatusCode::from_u16(response.status().as_u16()).unwrap();
            let headers = response
                .headers()
                .iter()
                .map(|(h, v)| {
                    let value_string = v.to_str().unwrap().to_owned();
                    // This should always be valid since we are coming from http.
                    let name = http::header::HeaderName::from_bytes(h.as_ref())
                        .expect("Should always be a valid header");
                    (name, value_string)
                })
                .collect();

            let body = response
                .into_body()
                // Unfortunate copy but we can fix this once we upgrade to newer
                // rusoto.
                .map_ok(|b| bytes::Bytes::from(&b[..]))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                .compat();

            Ok(HttpResponse {
                status,
                headers,
                body: ByteStream::new(body),
            })
        });

        (fut as BoxFuture<'static, Result<HttpResponse, HttpDispatchError>>).compat()
    }
}

impl Body for RusotoBody {
    type Data = io::Cursor<Vec<u8>>;
    type Error = io::Error;

    fn poll_data(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        match &mut self.inner {
            Some(SignedRequestPayload::Buffer(buf)) => {
                if !buf.is_empty() {
                    let buf = buf.split_off(0);
                    Poll::Ready(Some(Ok(io::Cursor::new(buf.into_iter().collect()))))
                } else {
                    Poll::Ready(None)
                }
            }
            Some(SignedRequestPayload::Stream(stream)) => {
                match with_notify(cx, || stream.poll())? {
                    Async::Ready(Some(buffer)) => {
                        Poll::Ready(Some(Ok(io::Cursor::new(buffer.into_iter().collect()))))
                    }
                    Async::Ready(None) => Poll::Ready(None),
                    Async::NotReady => Poll::Pending,
                }
            }
            None => Poll::Ready(None),
        }
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }
}

impl From<Option<SignedRequestPayload>> for RusotoBody {
    fn from(inner: Option<SignedRequestPayload>) -> Self {
        RusotoBody { inner }
    }
}
