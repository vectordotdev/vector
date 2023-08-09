use std::task::{Context, Poll};

use bytes::{Buf, Bytes, BytesMut};
use http::Request;
use hyper::{body, Body, Response};
use tower_http::decompression::DecompressionLayer;

use crate::{http::HttpClient, internal_events::EndpointBytesSent, sinks::prelude::*};

use super::request::HttpRequest;

/// Response type for use in the `Service` implementation of HTTP stream sinks.
pub struct HttpResponse {
    pub(super) http_response: Response<Bytes>,
    events_byte_size: GroupedCountByteSize,
    raw_byte_size: usize,
}

impl DriverResponse for HttpResponse {
    fn event_status(&self) -> EventStatus {
        if self.http_response.status().is_success() {
            EventStatus::Delivered
        } else {
            EventStatus::Rejected
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.raw_byte_size)
    }
}

///
pub trait HttpServiceRequestBuilder {
    /// B
    fn build(&self, body: BytesMut) -> Request<Bytes>;
}

/// `Service` implementation of HTTP stream sinks.
///
#[derive(Debug, Clone)]
pub struct HttpService<R> {
    http_request_builder: R,
    client: HttpClient,
    protocol: String,
}

impl<R> HttpService<R> {
    pub const fn new(http_request_builder: R, client: HttpClient, protocol: String) -> Self {
        Self {
            http_request_builder,
            client,
            protocol,
        }
    }
}

impl<R> Service<HttpRequest> for HttpService<R>
where
    R: HttpServiceRequestBuilder,
{
    type Response = HttpResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: HttpRequest) -> Self::Future {
        // for internal metrics reporting
        let raw_byte_size = request.payload.len();
        let events_byte_size = request
            .request_metadata
            .into_events_estimated_json_encoded_byte_size();

        // build the request
        let mut bytes_mut = BytesMut::new();
        bytes_mut.extend(request.payload);
        let http_request = self.http_request_builder.build(bytes_mut);
        let req = http_request.map(Body::from);

        // for internal metrics reporting
        let endpoint = req.uri().to_string();
        let protocol = self.protocol.clone();

        let mut decompression_service = ServiceBuilder::new()
            .layer(DecompressionLayer::new())
            .service(self.client.clone());

        Box::pin(async move {
            let response = decompression_service.call(req).await?;

            if response.status().is_success() {
                emit!(EndpointBytesSent {
                    byte_size: raw_byte_size,
                    protocol: protocol.as_str(),
                    endpoint: endpoint.as_str(),
                });
            }

            let (parts, body) = response.into_parts();
            let mut body = body::aggregate(body).await?;
            let http_response = Response::from_parts(parts, body.copy_to_bytes(body.remaining()));

            Ok(HttpResponse {
                http_response,
                events_byte_size,
                raw_byte_size,
            })
        })
    }
}
