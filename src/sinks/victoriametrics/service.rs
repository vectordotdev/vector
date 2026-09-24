use std::{
    collections::BTreeMap,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use http::{HeaderValue, Method, Uri, header};

use super::{Tenant, request_builder::VmRequest};
use crate::{
    http::{Auth, HttpClient},
    internal_events::EndpointBytesSent,
    sinks::{
        prelude::*,
        util::{
            HttpEndpoint,
            http::{HttpResponse, OrderedHeaderName},
        },
    },
};

const REMOTE_WRITE_VERSION_HEADER: &str = "X-VictoriaMetrics-Remote-Write-Version";
const REMOTE_WRITE_VERSION: &str = "1";
const CONTENT_TYPE_PROTOBUF: &str = "application/x-protobuf";
const CONTENT_ENCODING_ZSTD: &str = "zstd";

/// The write path of single-node VictoriaMetrics, and of `vmauth` in front of a cluster.
pub(super) const WRITE_PATH: &str = "/api/v1/write";
/// The `vminsert` write path that reads tenants from `vm_account_id`/`vm_project_id` labels.
pub(super) const MULTITENANT_WRITE_PATH: &str = "/insert/multitenant/prometheus/api/v1/write";

/// Where requests are sent.
#[derive(Clone, Debug)]
pub(super) enum WriteEndpoint {
    /// A single URL for every request.
    Static(Uri),
    /// `<base>/insert/<tenant>/prometheus/api/v1/write`, per request.
    PerTenant(HttpEndpoint),
}

impl WriteEndpoint {
    pub(super) fn resolve(&self, tenant: Option<&Tenant>) -> crate::Result<Uri> {
        match (self, tenant) {
            (Self::Static(uri), _) => Ok(uri.clone()),
            (Self::PerTenant(base), Some(tenant)) => Ok(base
                .append_path(&tenant_write_path(tenant))?
                .as_uri()
                .clone()),
            (Self::PerTenant(_), None) => {
                Err("A tenant is required to build the write URL.".into())
            }
        }
    }
}

pub(super) fn tenant_write_path(tenant: &Tenant) -> String {
    format!("/insert/{}/prometheus/api/v1/write", tenant.path())
}

/// Headers and credentials shared by every request.
#[derive(Clone, Debug)]
pub(super) struct RequestSettings {
    pub(super) auth: Option<Auth>,
    pub(super) headers: Arc<BTreeMap<OrderedHeaderName, HeaderValue>>,
}

impl RequestSettings {
    /// Builds a write request. A per-event `token` replaces the configured `auth`.
    pub(super) fn build_request(
        &self,
        method: Method,
        uri: Uri,
        body: Bytes,
        token: Option<&str>,
    ) -> crate::Result<http::Request<hyper::Body>> {
        let mut builder = http::Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, CONTENT_TYPE_PROTOBUF)
            .header(header::CONTENT_ENCODING, CONTENT_ENCODING_ZSTD)
            .header(REMOTE_WRITE_VERSION_HEADER, REMOTE_WRITE_VERSION);

        for (name, value) in self.headers.iter() {
            builder = builder.header(name.inner(), value);
        }

        let mut request = builder.body(body)?;

        match (token, &self.auth) {
            (Some(token), _) => {
                let mut value = HeaderValue::from_str(&format!("Bearer {token}")).map_err(
                    |_| "The `victoriametrics_token` secret is not a valid header value.",
                )?;
                value.set_sensitive(true);
                request.headers_mut().insert(header::AUTHORIZATION, value);
            }
            (None, Some(auth)) => auth.apply(&mut request),
            (None, None) => {}
        }

        Ok(request.map(hyper::Body::from))
    }
}

#[derive(Clone)]
pub(super) struct VmService {
    pub(super) client: HttpClient,
    pub(super) endpoint: WriteEndpoint,
    pub(super) settings: RequestSettings,
}

impl Service<VmRequest> for VmService {
    type Response = HttpResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, mut request: VmRequest) -> Self::Future {
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        let settings = self.settings.clone();

        Box::pin(async move {
            let metadata = std::mem::take(request.metadata_mut());
            let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
            let raw_byte_size = request.body.len();

            let uri = endpoint.resolve(request.key.tenant.as_ref())?;
            let http_request = settings.build_request(
                Method::POST,
                uri.clone(),
                request.body,
                request.key.token.as_deref(),
            )?;

            let response = client.send(http_request).await?;
            let (parts, body) = response.into_parts();
            let body = http_body::Body::collect(body).await?.to_bytes();
            let http_response = hyper::Response::from_parts(parts, body);

            if http_response.status().is_success() {
                // Emitted here rather than by the driver because it carries the `endpoint` tag.
                emit!(EndpointBytesSent {
                    byte_size: raw_byte_size,
                    protocol: "http",
                    endpoint: &uri.to_string(),
                });
            }

            Ok(HttpResponse {
                http_response,
                events_byte_size,
                raw_byte_size,
            })
        })
    }
}
