use futures_util::{future::BoxFuture, task::Poll};
use http::{Request, Uri};
use hyper::Body;
use tower::Service;
use vector_lib::request_metadata::MetaDescriptive;

use crate::{
    gcp::GcpAuthenticator,
    http::HttpClient,
    sinks::{
        gcp_chronicle::{
            GcsHealthcheckError, ChronicleRequest, ChronicleResponseError
        },
        gcs_common::{
            config::healthcheck_response,
            service::GcsResponse,
        },
        Healthcheck,
    },
};

#[derive(Debug, Clone)]
pub struct ChronicleService {
    client: HttpClient,
    base_url: String,
    creds: GcpAuthenticator,
}

impl ChronicleService {
    pub const fn new(client: HttpClient, base_url: String, creds: GcpAuthenticator) -> Self {
        Self {
            client,
            base_url,
            creds,
        }
    }
}

impl Service<ChronicleRequest> for ChronicleService {
    type Response = GcsResponse;
    type Error = ChronicleResponseError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: ChronicleRequest) -> Self::Future {
        let mut builder = Request::post(&self.base_url);
        let metadata = request.get_metadata().clone();
        let headers = builder.headers_mut().unwrap();

        for (name, value) in request.headers {
            headers.insert(name, value);
        }

        let mut http_request = builder.body(Body::from(request.body)).unwrap();
        self.creds.apply(&mut http_request);

        let mut client = self.client.clone();
        Box::pin(async move {
            match client.call(http_request).await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        Ok(GcsResponse {
                            inner: response,
                            metadata,
                        })
                    } else {
                        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
                        Err(ChronicleResponseError::ServerError {
                            code: status,
                            message: String::from_utf8(body.to_vec()).unwrap()
                        })
                    }
                }
                Err(error) => Err(ChronicleResponseError::HttpError { error }),
            }
        })
    }
}

pub fn build_healthcheck(
    client: HttpClient,
    base_url: &str,
    auth: GcpAuthenticator,
) -> crate::Result<Healthcheck> {
    let uri = base_url.parse::<Uri>()?;

    let healthcheck = async move {
        let mut request = http::Request::get(&uri).body(Body::empty())?;
        auth.apply(&mut request);

        let response = client.send(request).await?;
        healthcheck_response(response, GcsHealthcheckError::NotFound.into())
    };

    Ok(Box::pin(healthcheck))
}
