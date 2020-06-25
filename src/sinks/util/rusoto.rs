#![cfg(feature = "rusoto_core")]

use crate::{dns::Resolver, sinks::util, tls::MaybeTlsSettings};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use http::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method, Request, Response, StatusCode,
};
use hyper::body::{Body, HttpBody};
use rusoto_core::{
    request::{
        DispatchSignedRequest, DispatchSignedRequestFuture, HttpDispatchError, HttpResponse,
    },
    ByteStream, Region,
};
use rusoto_credential::{
    AutoRefreshingProvider, AwsCredentials, ChainProvider, CredentialsError, ProvideAwsCredentials,
    StaticProvider,
};
use rusoto_signature::{SignedRequest, SignedRequestPayload};
use rusoto_sts::{StsAssumeRoleSessionCredentialsProvider, StsClient};
use snafu::{ResultExt, Snafu};
use std::{
    fmt, io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tower03::{Service, ServiceExt};

pub type Client = HttpClient<util::http::HttpClient<RusotoBody>>;

pub fn client(resolver: Resolver) -> crate::Result<Client> {
    let settings = MaybeTlsSettings::enable_client()?;
    let client = util::http::HttpClient::new(resolver, settings)?;
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

impl fmt::Debug for AwsCredentialsProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Default(_) => "default",
            Self::Role(_) => "role",
            Self::Static(_) => "static",
        };

        f.debug_tuple("AwsCredentialsProvider")
            .field(&name)
            .finish()
    }
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

#[async_trait]
impl ProvideAwsCredentials for AwsCredentialsProvider {
    async fn credentials(&self) -> Result<AwsCredentials, CredentialsError> {
        let fut = match self {
            Self::Default(p) => p.credentials(),
            Self::Role(p) => p.credentials(),
            Self::Static(p) => p.credentials(),
        };
        fut.await
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
    T: Service<Request<RusotoBody>, Response = Response<Body>, Error = hyper::Error>
        + Clone
        + Send
        + 'static,
    T::Future: Send + 'static,
{
    // Adaptation of https://docs.rs/rusoto_core/0.44.0/src/rusoto_core/request.rs.html#314-416
    fn dispatch(
        &self,
        request: SignedRequest,
        timeout: Option<Duration>,
    ) -> DispatchSignedRequestFuture {
        assert!(timeout.is_none(), "timeout is not supported at this level");

        let client = self.client.clone();

        Box::pin(async move {
            let method = match request.method().as_ref() {
                "POST" => Method::POST,
                "PUT" => Method::PUT,
                "DELETE" => Method::DELETE,
                "GET" => Method::GET,
                "HEAD" => Method::HEAD,
                v => {
                    return Err(HttpDispatchError::new(format!(
                        "Unsupported HTTP verb {}",
                        v
                    )));
                }
            };

            let mut headers = HeaderMap::new();
            for h in request.headers().iter() {
                let header_name = match h.0.parse::<HeaderName>() {
                    Ok(name) => name,
                    Err(err) => {
                        return Err(HttpDispatchError::new(format!(
                            "error parsing header name: {}",
                            err
                        )))
                    }
                };
                for v in h.1.iter() {
                    let header_value = match HeaderValue::from_bytes(v) {
                        Ok(value) => value,
                        Err(err) => {
                            return Err(HttpDispatchError::new(format!(
                                "error parsing header value: {}",
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
                .map_err(|e| format!("error building request: {}", e))
                .map_err(HttpDispatchError::new)?;

            *request.headers_mut() = headers;

            let response = client
                .oneshot(request)
                .await
                .map_err(|e| HttpDispatchError::new(format!("Error during dispatch: {}", e)))?;

            let status = StatusCode::from_u16(response.status().as_u16()).unwrap();
            let headers = response
                .headers()
                .iter()
                .map(|(h, v)| {
                    let value_string = v.to_str().unwrap().to_owned();
                    // This should always be valid since we are coming from http.
                    let name = HeaderName::from_bytes(h.as_ref())
                        .expect("Should always be a valid header");
                    (name, value_string)
                })
                .collect();

            let body = response.into_body().map(|try_chunk| {
                try_chunk
                    .map(|c| c)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            });

            Ok(HttpResponse {
                status,
                headers,
                body: ByteStream::new(body),
            })
        })
    }
}

impl HttpBody for RusotoBody {
    type Data = io::Cursor<Vec<u8>>;
    type Error = io::Error;

    fn poll_data(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        match self.inner.as_mut() {
            Some(SignedRequestPayload::Buffer(buf)) => {
                if !buf.is_empty() {
                    let buf = buf.split_off(0);
                    Poll::Ready(Some(Ok(io::Cursor::new(buf.into_iter().collect()))))
                } else {
                    Poll::Ready(None)
                }
            }
            Some(SignedRequestPayload::Stream(stream)) => {
                let stream = Pin::new(stream);
                match stream.poll_next(cx) {
                    Poll::Ready(Some(result)) => match result {
                        Ok(buf) => {
                            Poll::Ready(Some(Ok(io::Cursor::new(buf.into_iter().collect()))))
                        }
                        Err(error) => Poll::Ready(Some(Err(error))),
                    },
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
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
