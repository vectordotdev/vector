use std::{
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use http_1::{HeaderName, HeaderValue, Method, Request, Response, Uri};
use http_body_util::BodyExt;
use snafu::ResultExt;
use vector_http::full_body;

use super::request_builder::RemoteWriteRequest;
use crate::{
    http::client_v1::HttpClient,
    internal_events::EndpointBytesSent,
    sinks::{
        HTTPV1RequestBuilderSnafu,
        prelude::*,
        util::{auth::Auth, http_v1::HttpResponse},
    },
};

/// Constants for header strings.
mod headers {
    pub(super) const X_PROMETHEUS_REMOTE_WRITE_VERSION: &str = "X-Prometheus-Remote-Write-Version";
    pub(super) const CONTENT_ENCODING: &str = "Content-Encoding";
    pub(super) const CONTENT_TYPE: &str = "Content-Type";
    pub(super) const X_SCOPE_ORGID: &str = "X-Scope-OrgID";

    pub(super) const VERSION: &str = "0.1.0";
    pub(super) const APPLICATION_X_PROTOBUF: &str = "application/x-protobuf";
}

/// Validated custom request headers, in the sorted order of `validate_headers`.
///
/// They are appended after the protocol headers, so a custom header with the same name adds
/// a second value instead of replacing the protocol one.
pub(super) type CustomHeaders = Arc<[(HeaderName, HeaderValue)]>;

#[derive(Clone)]
pub(super) struct RemoteWriteService {
    pub(super) endpoint: Uri,
    pub(super) auth: Option<Auth>,
    pub(super) client: HttpClient,
    pub(super) compression: super::Compression,
    pub(super) headers: CustomHeaders,
}

impl Service<RemoteWriteRequest> for RemoteWriteService {
    type Response = HttpResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _task: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, mut request: RemoteWriteRequest) -> Self::Future {
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        let auth = self.auth.clone();
        let compression = self.compression;
        let headers = Arc::clone(&self.headers);

        Box::pin(async move {
            let metadata = std::mem::take(request.metadata_mut());
            let json_size = metadata.into_events_estimated_json_encoded_byte_size();
            let raw_byte_size = request.request.len();

            let http_request = build_request(
                Method::POST,
                &endpoint,
                compression,
                request.request,
                request.tenant_id.as_ref(),
                auth.as_ref(),
                &headers,
            )
            .await?;

            let response = client.send(http_request.map(full_body)).await?;
            let (parts, body) = response.into_parts();
            let body = body.collect().await?.to_bytes();
            let response = Response::from_parts(parts, body);

            if response.status().is_success() {
                // We can't rely on the framework to emit this because we need to specify the additional `endpoint` tag.
                emit!(EndpointBytesSent {
                    byte_size: raw_byte_size,
                    protocol: "http",
                    endpoint: &endpoint.to_string(),
                });
            }

            Ok(HttpResponse {
                response,
                events_byte_size: json_size,
                raw_byte_size,
            })
        })
    }
}

/// Builds a remote write request with the protocol, tenant, custom and auth headers.
///
/// AWS authentication signs the request, so callers must not change it afterwards except to
/// convert the body at the client boundary.
pub(super) async fn build_request(
    method: Method,
    endpoint: &Uri,
    compression: Compression,
    body: Bytes,
    tenant_id: Option<&String>,
    auth: Option<&Auth>,
    custom_headers: &[(HeaderName, HeaderValue)],
) -> crate::Result<Request<Bytes>> {
    let mut builder = Request::builder()
        .method(method)
        .uri(endpoint)
        .header(headers::X_PROMETHEUS_REMOTE_WRITE_VERSION, headers::VERSION)
        .header(headers::CONTENT_TYPE, headers::APPLICATION_X_PROTOBUF);

    if let Some(content_encoding) = compression.content_encoding() {
        builder = builder.header(headers::CONTENT_ENCODING, content_encoding);
    }

    if let Some(tenant_id) = tenant_id {
        builder = builder.header(headers::X_SCOPE_ORGID, tenant_id);
    }

    for (name, value) in custom_headers {
        builder = builder.header(name, value);
    }

    let mut request = builder.body(body).context(HTTPV1RequestBuilderSnafu)?;

    match auth {
        Some(Auth::Basic(http_auth)) => http_auth.apply_v1(&mut request),
        #[cfg(feature = "aws-core")]
        Some(Auth::Aws {
            credentials_provider,
            region,
        }) => {
            crate::aws::sign_request_v1(
                "aps",
                &mut request,
                credentials_provider,
                Some(region),
                false,
            )
            .await?;
        }
        None => {}
    }

    Ok(request)
}
