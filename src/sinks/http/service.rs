//! Service implementation for the `http` sink.

use std::{
    io::Write,
    task::{Context, Poll},
};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use codecs::{
    encoding::{Framer, Serializer},
    CharacterDelimitedEncoder,
};
use http::{HeaderName, HeaderValue, Method, Request, StatusCode, Uri};
use hyper::{body, Body, Response};
use indexmap::IndexMap;
use tower_http::decompression::DecompressionLayer;

use crate::{
    http::{Auth, HttpClient, HttpError},
    internal_events::EndpointBytesSent,
    sinks::{
        prelude::*,
        util::{Compressor, UriSerde},
    },
};

use super::config::HttpMethod;

pub struct HttpResponse {
    http_response: Response<Bytes>,
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

#[derive(Clone)]
pub(super) struct HttpRequest {
    payload: Bytes,
    finalizers: EventFinalizers,
    request_metadata: RequestMetadata,
}

impl HttpRequest {
    pub(super) fn new(
        payload: Bytes,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
    ) -> Self {
        Self {
            payload,
            finalizers,
            request_metadata,
        }
    }
}

impl Finalizable for HttpRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for HttpRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

#[derive(Debug, Default, Clone)]
pub struct HttpRetryLogic;

impl RetryLogic for HttpRetryLogic {
    type Error = HttpError;
    type Response = HttpResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let response = &response.http_response;
        let status = response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(
                format!("{}: {}", status, String::from_utf8_lossy(response.body())).into(),
            ),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct HttpService {
    uri: UriSerde,
    method: HttpMethod,
    auth: Option<Auth>,
    headers: IndexMap<HeaderName, HeaderValue>,
    payload_prefix: String,
    payload_suffix: String,
    compression: Compression,
    encoder: Encoder<Framer>,
    client: HttpClient,
    protocol: String,
}

impl HttpService {
    pub(super) fn new(
        uri: UriSerde,
        method: HttpMethod,
        auth: Option<Auth>,
        headers: IndexMap<HeaderName, HeaderValue>,
        payload_prefix: String,
        payload_suffix: String,
        compression: Compression,
        encoder: Encoder<Framer>,
        client: HttpClient,
        protocol: String,
    ) -> Self {
        Self {
            uri,
            method,
            auth,
            headers,
            payload_prefix,
            payload_suffix,
            compression,
            encoder,
            client,
            protocol,
        }
    }

    fn build_http_request(&self, mut body: BytesMut) -> Request<Bytes> {
        let method: Method = self.method.into();
        let uri: Uri = self.uri.uri.clone();

        let content_type = {
            use Framer::*;
            use Serializer::*;
            match (self.encoder.serializer(), self.encoder.framer()) {
                (RawMessage(_) | Text(_), _) => Some("text/plain"),
                (Json(_), NewlineDelimited(_)) => {
                    if !body.is_empty() {
                        // Remove trailing newline for backwards-compatibility
                        // with Vector `0.20.x`.
                        body.truncate(body.len() - 1);
                    }
                    Some("application/x-ndjson")
                }
                (Json(_), CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' })) => {
                    // TODO(https://github.com/vectordotdev/vector/issues/11253):
                    // Prepend before building a request body to eliminate the
                    // additional copy here.
                    let message = body.split();
                    body.put(self.payload_prefix.as_bytes());
                    body.put_u8(b'[');
                    if !message.is_empty() {
                        body.unsplit(message);
                        // remove trailing comma from last record
                        body.truncate(body.len() - 1);
                    }
                    body.put_u8(b']');
                    body.put(self.payload_suffix.as_bytes());
                    Some("application/json")
                }
                _ => None,
            }
        };

        let mut builder = Request::builder().method(method).uri(uri);

        if let Some(content_type) = content_type {
            builder = builder.header("Content-Type", content_type);
        }

        let compression = self.compression;

        if compression.is_compressed() {
            builder = builder.header(
                "Content-Encoding",
                compression
                    .content_encoding()
                    .expect("Encoding should be specified."),
            );

            let mut compressor = Compressor::from(compression);
            compressor
                .write_all(&body)
                .expect("Writing to Vec can't fail.");
            body = compressor.finish().expect("Writing to Vec can't fail.");
        }

        let headers = builder
            .headers_mut()
            // The request building should not have errors at this point, and if it did it would fail in the call to `body()` also.
            .expect("Failed to access headers in http::Request builder- builder has errors.");
        for (header, value) in self.headers.iter() {
            headers.insert(header, value.clone());
        }

        let mut request = builder.body(body.freeze()).unwrap();

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        request
    }
}

impl Service<HttpRequest> for HttpService {
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
        let http_request = self.build_http_request(bytes_mut);
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
