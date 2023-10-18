use std::task::{Context, Poll};

#[cfg(feature = "aws-core")]
use aws_credential_types::provider::SharedCredentialsProvider;
#[cfg(feature = "aws-core")]
use aws_types::region::Region;

use bytes::Bytes;
use http::Uri;

use super::request_builder::RemoteWriteRequest;
use crate::{
    http::HttpClient,
    internal_events::EndpointBytesSent,
    sinks::{
        prelude::*,
        util::{auth::Auth, http::HttpResponse},
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

#[derive(Clone)]
pub(super) struct RemoteWriteService {
    pub(super) endpoint: Uri,
    pub(super) auth: Option<Auth>,
    pub(super) client: HttpClient,
    pub(super) compression: super::Compression,
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

        Box::pin(async move {
            let metadata = std::mem::take(request.metadata_mut());
            let json_size = metadata.into_events_estimated_json_encoded_byte_size();
            let raw_byte_size = request.request.len();

            let http_request = build_request(
                http::Method::POST,
                &endpoint,
                compression,
                request.request,
                request.tenant_id.as_ref(),
                auth,
            )
            .await?;

            let response = client.send(http_request).await?;
            let (parts, body) = response.into_parts();
            let body = hyper::body::to_bytes(body).await?;
            let http_response = hyper::Response::from_parts(parts, body);

            if http_response.status().is_success() {
                // We can't rely on the framework to emit this because we need to specify the additional `endpoint` tag.
                emit!(EndpointBytesSent {
                    byte_size: raw_byte_size,
                    protocol: "http",
                    endpoint: &endpoint.to_string(),
                });
            }

            Ok(HttpResponse {
                events_byte_size: json_size,
                http_response,
                raw_byte_size,
            })
        })
    }
}

#[cfg(feature = "aws-core")]
async fn sign_request(
    request: &mut http::Request<Bytes>,
    credentials_provider: &SharedCredentialsProvider,
    region: &Option<Region>,
) -> crate::Result<()> {
    crate::aws::sign_request("aps", request, credentials_provider, region).await
}

pub(super) async fn build_request(
    method: http::Method,
    endpoint: &Uri,
    compression: Compression,
    body: Bytes,
    tenant_id: Option<&String>,
    auth: Option<Auth>,
) -> crate::Result<http::Request<hyper::Body>> {
    let mut builder = http::Request::builder()
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

    let mut request = builder.body(body)?;

    if let Some(auth) = auth {
        match auth {
            Auth::Basic(http_auth) => http_auth.apply(&mut request),
            #[cfg(feature = "aws-core")]
            Auth::Aws {
                credentials_provider: provider,
                region,
            } => sign_request(&mut request, &provider, &Some(region.clone())).await?,
        }
    }

    Ok(request.map(hyper::Body::from))
}
