//! Service implementation for the `http` sink.

use bytes::Bytes;
use http::{HeaderName, HeaderValue, Method, Request, Uri};
use indexmap::IndexMap;

use crate::{
    http::Auth,
    sinks::util::{http::HttpServiceRequestBuilder, UriSerde},
};

use super::config::HttpMethod;

#[derive(Debug, Clone)]
pub(super) struct HttpSinkRequestBuilder {
    uri: UriSerde,
    method: HttpMethod,
    auth: Option<Auth>,
    headers: IndexMap<HeaderName, HeaderValue>,
    content_type: Option<String>,
    content_encoding: Option<String>,
}

impl HttpSinkRequestBuilder {
    /// Creates a new `HttpSinkRequestBuilder`
    pub(super) const fn new(
        uri: UriSerde,
        method: HttpMethod,
        auth: Option<Auth>,
        headers: IndexMap<HeaderName, HeaderValue>,
        content_type: Option<String>,
        content_encoding: Option<String>,
    ) -> Self {
        Self {
            uri,
            method,
            auth,
            headers,
            content_type,
            content_encoding,
        }
    }
}

impl HttpServiceRequestBuilder for HttpSinkRequestBuilder {
    fn build(&self, body: Bytes) -> Request<Bytes> {
        let method: Method = self.method.into();
        let uri: Uri = self.uri.uri.clone();
        let mut builder = Request::builder().method(method).uri(uri);

        if let Some(content_type) = &self.content_type {
            builder = builder.header("Content-Type", content_type);
        }

        if let Some(content_encoding) = &self.content_encoding {
            builder = builder.header("Content-Encoding", content_encoding);
        }

        let headers = builder
            .headers_mut()
            // The request building should not have errors at this point, and if it did it would fail in the call to `body()` also.
            .expect("Failed to access headers in http::Request builder- builder has errors.");

        for (header, value) in self.headers.iter() {
            headers.insert(header, value.clone());
        }

        // The request building should not have errors at this point
        let mut request = builder
            .body(body)
            .expect("Failed to assign body to request- builder has errors");

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        request
    }
}
