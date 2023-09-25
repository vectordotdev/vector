use std::task::{Context, Poll};

#[cfg(feature = "aws-core")]
use aws_credential_types::provider::SharedCredentialsProvider;
#[cfg(feature = "aws-core")]
use aws_types::region::Region;

use bytes::Bytes;
use http::{StatusCode, Uri};

use super::request_builder::RemoteWriteRequest;
use crate::{
    http::{HttpClient, HttpError},
    internal_events::EndpointBytesSent,
    sinks::{prelude::*, util::auth::Auth},
};

#[derive(Debug, Default, Clone)]
pub(super) struct RemoteWriteRetryLogic;

impl RetryLogic for RemoteWriteRetryLogic {
    type Error = HttpError;
    type Response = RemoteWriteResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(
                format!(
                    "{}: {}",
                    status,
                    String::from_utf8_lossy(response.response.body())
                )
                .into(),
            ),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}

#[derive(Clone)]
pub(super) struct RemoteWriteService {
    pub(super) endpoint: Uri,
    pub(super) auth: Option<Auth>,
    pub(super) client: HttpClient,
    pub(super) compression: super::Compression,
}

pub(super) struct RemoteWriteResponse {
    json_size: GroupedCountByteSize,
    response: http::Response<Bytes>,
}

impl DriverResponse for RemoteWriteResponse {
    fn event_status(&self) -> EventStatus {
        if self.response.status().is_success() {
            EventStatus::Delivered
        } else {
            EventStatus::Errored
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.json_size
    }
}

impl Service<RemoteWriteRequest> for RemoteWriteService {
    type Response = RemoteWriteResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _task: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, mut request: RemoteWriteRequest) -> Self::Future {
        let client = self.client.clone();
        let metadata = std::mem::take(request.metadata_mut());
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
        let endpoint = self.endpoint.clone();
        let auth = self.auth.clone();
        let compression = self.compression;

        Box::pin(async move {
            let request = build_request(
                http::Method::POST,
                &endpoint,
                compression,
                request.request,
                request.tenant_id.as_ref(),
                auth,
            )
            .await?;

            let (parts, body) = request.into_parts();
            let request: hyper::Request<hyper::Body> =
                hyper::Request::from_parts(parts, body.into());

            let response = client.send(request.map(hyper::Body::from)).await?;
            let (parts, body) = response.into_parts();
            let body = hyper::body::to_bytes(body).await?;
            let byte_size = body.len();

            let response = hyper::Response::from_parts(parts, body);

            if response.status().is_success() {
                // We can't rely on the framework to emit this because we need to specify the additional `endpoint` tag.
                emit!(EndpointBytesSent {
                    byte_size,
                    protocol: "http",
                    endpoint: &endpoint.to_string(),
                });
            }

            Ok(RemoteWriteResponse {
                json_size: events_byte_size,
                response,
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
    compression: super::Compression,
    body: Bytes,
    tenant_id: Option<&String>,
    auth: Option<Auth>,
) -> crate::Result<http::Request<Bytes>> {
    let content_encoding = convert_compression_to_content_encoding(compression);

    let mut builder = http::Request::builder()
        .method(method)
        .uri(endpoint)
        .header("X-Prometheus-Remote-Write-Version", "0.1.0")
        .header("Content-Encoding", content_encoding)
        .header("Content-Type", "application/x-protobuf");

    if let Some(tenant_id) = tenant_id {
        builder = builder.header("X-Scope-OrgID", tenant_id);
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

    Ok(request)
}

const fn convert_compression_to_content_encoding(compression: super::Compression) -> &'static str {
    match compression {
        super::Compression::Snappy => "snappy",
        super::Compression::Gzip => "gzip",
        super::Compression::Zstd => "zstd",
    }
}
