use std::{
    collections::HashMap,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::{header::HeaderName, Response, Uri};
use hyper::{header::HeaderValue, service::Service, Body, Request};
use rusoto_core::{
    credential::{AwsCredentials, ProvideAwsCredentials},
    signature::{SignedRequest, SignedRequestPayload},
    Region,
};
use tower::ServiceExt;
use vector_core::{
    buffers::Ackable, internal_event::EventsSent, stream::DriverResponse, ByteSizeOf,
};

use crate::{
    aws::rusoto::AwsCredentialsProvider,
    event::{EventFinalizers, EventStatus, Finalizable},
    http::{Auth, HttpClient},
    sinks::util::{
        http::{HttpBatchService, RequestConfig},
        Compression, ElementCount,
    },
};

#[derive(Clone)]
pub struct ElasticSearchRequest {
    pub payload: Vec<u8>,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
    pub events_byte_size: usize,
}

impl ByteSizeOf for ElasticSearchRequest {
    fn allocated_bytes(&self) -> usize {
        self.payload.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for ElasticSearchRequest {
    fn element_count(&self) -> usize {
        self.batch_size
    }
}

impl Ackable for ElasticSearchRequest {
    fn ack_size(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for ElasticSearchRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

#[derive(Clone)]
pub struct ElasticSearchService {
    batch_service: HttpBatchService<
        BoxFuture<'static, Result<http::Request<Vec<u8>>, crate::Error>>,
        ElasticSearchRequest,
    >,
}

impl ElasticSearchService {
    pub fn new(
        http_client: HttpClient<Body>,
        http_request_builder: HttpRequestBuilder,
    ) -> ElasticSearchService {
        let http_request_builder = Arc::new(http_request_builder);
        let batch_service = HttpBatchService::new(http_client, move |req| {
            let request_builder = Arc::clone(&http_request_builder);
            let future: BoxFuture<'static, Result<http::Request<Vec<u8>>, crate::Error>> =
                Box::pin(async move { request_builder.build_request(req).await });
            future
        });
        ElasticSearchService { batch_service }
    }
}

pub struct HttpRequestBuilder {
    pub bulk_uri: Uri,
    pub query_params: HashMap<String, String>,
    pub region: Region,
    pub compression: Compression,
    pub http_request_config: RequestConfig,
    pub http_auth: Option<Auth>,
    pub credentials_provider: Option<AwsCredentialsProvider>,
}

impl HttpRequestBuilder {
    pub async fn build_request(
        &self,
        es_req: ElasticSearchRequest,
    ) -> Result<Request<Vec<u8>>, crate::Error> {
        let mut builder = Request::post(&self.bulk_uri);

        let request = if let Some(credentials_provider) = &self.credentials_provider {
            let mut request = self.create_signed_request("POST", &self.bulk_uri, true);
            let aws_credentials = credentials_provider.credentials().await?;

            request.add_header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression.content_encoding() {
                request.add_header("Content-Encoding", ce);
            }

            for (header, value) in &self.http_request_config.headers {
                request.add_header(header, value);
            }

            request.set_payload(Some(es_req.payload));
            builder = sign_request(&mut request, &aws_credentials, builder);

            // The SignedRequest ends up owning the body, so we have
            // to play games here
            let body = request.payload.take().unwrap();
            match body {
                SignedRequestPayload::Buffer(body) => builder
                    .body(body.to_vec())
                    .expect("Invalid http request value used"),
                _ => unreachable!(),
            }
        } else {
            builder = builder.header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression.content_encoding() {
                builder = builder.header("Content-Encoding", ce);
            }

            for (header, value) in &self.http_request_config.headers {
                builder = builder.header(&header[..], &value[..]);
            }

            if let Some(auth) = &self.http_auth {
                builder = auth.apply_builder(builder);
            }

            builder
                .body(es_req.payload)
                .expect("Invalid http request value used")
        };
        Ok(request)
    }

    fn create_signed_request(&self, method: &str, uri: &Uri, use_params: bool) -> SignedRequest {
        let mut request = SignedRequest::new(method, "es", &self.region, uri.path());
        request.set_hostname(uri.host().map(|host| host.into()));
        if use_params {
            for (key, value) in &self.query_params {
                request.add_param(key, value);
            }
        }
        request
    }
}

fn sign_request(
    request: &mut SignedRequest,
    credentials: &AwsCredentials,
    mut builder: http::request::Builder,
) -> http::request::Builder {
    request.sign(credentials);

    for (name, values) in request.headers() {
        let header_name = name
            .parse::<HeaderName>()
            .expect("Could not parse header name.");
        for value in values {
            let header_value =
                HeaderValue::from_bytes(value).expect("Could not parse header value.");
            builder = builder.header(&header_name, header_value);
        }
    }
    builder
}

pub struct ElasticSearchResponse {
    pub http_response: Response<Bytes>,
    pub event_status: EventStatus,
    pub batch_size: usize,
    pub events_byte_size: usize,
}

impl DriverResponse for ElasticSearchResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.batch_size,
            byte_size: self.events_byte_size,
            output: None,
        }
    }
}

impl Service<ElasticSearchRequest> for ElasticSearchService {
    type Response = ElasticSearchResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ElasticSearchRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();
        Box::pin(async move {
            http_service.ready().await?;
            let batch_size = req.batch_size;
            let events_byte_size = req.events_byte_size;
            let http_response = http_service.call(req).await?;
            let event_status = get_event_status(&http_response);
            Ok(ElasticSearchResponse {
                event_status,
                http_response,
                batch_size,
                events_byte_size,
            })
        })
    }
}

fn get_event_status(response: &Response<Bytes>) -> EventStatus {
    if response.status().is_success() {
        let body = String::from_utf8_lossy(response.body());
        if body.contains("\"errors\":true") {
            error!(message = "Response contained errors.", ?response);
            EventStatus::Rejected
        } else {
            trace!(message = "Response successful.", ?response);
            EventStatus::Delivered
        }
    } else if response.status().is_server_error() {
        error!(message = "Response wasn't successful.", ?response);
        EventStatus::Errored
    } else {
        error!(message = "Response failed.", ?response);
        EventStatus::Rejected
    }
}
