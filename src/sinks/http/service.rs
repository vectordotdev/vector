//! Service implementation for the `http` sink.

use std::collections::BTreeMap;
use std::str::FromStr;

use bytes::Bytes;
use http::{
    header::{CONTENT_ENCODING, CONTENT_TYPE},
    HeaderName, HeaderValue, Method, Request,
};

use crate::{
    http::{Auth, MaybeAuth},
    sinks::{
        util::{
            http::{HttpRequest, HttpServiceRequestBuilder, OrderedHeaderName},
            UriSerde,
        },
        HTTPRequestBuilderSnafu,
    },
};
use snafu::ResultExt;

use super::config::HttpMethod;
use super::sink::PartitionKey;

#[derive(Debug, Clone)]
pub(super) struct HttpSinkRequestBuilder {
    method: HttpMethod,
    auth: Option<Auth>,
    static_headers: BTreeMap<OrderedHeaderName, HeaderValue>,
    content_type: Option<String>,
    content_encoding: Option<String>,
}

impl HttpSinkRequestBuilder {
    /// Creates a new `HttpSinkRequestBuilder`
    pub(super) const fn new(
        method: HttpMethod,
        auth: Option<Auth>,
        static_headers: BTreeMap<OrderedHeaderName, HeaderValue>,
        content_type: Option<String>,
        content_encoding: Option<String>,
    ) -> Self {
        Self {
            method,
            auth,
            static_headers,
            content_type,
            content_encoding,
        }
    }
}

impl HttpServiceRequestBuilder<PartitionKey> for HttpSinkRequestBuilder {
    fn build(
        &self,
        mut request: HttpRequest<PartitionKey>,
    ) -> Result<Request<Bytes>, crate::Error> {
        let metadata = request.get_additional_metadata();
        let uri_serde = UriSerde::from_str(&metadata.uri)?;
        let uri_auth = uri_serde.auth;
        let uri = uri_serde.uri;

        let auth = self.auth.choose_one(&uri_auth)?;

        let method: Method = self.method.into();
        let mut builder = Request::builder().method(method).uri(uri);

        if let Some(content_type) = &self.content_type {
            builder = builder.header(CONTENT_TYPE, content_type);
        }

        if let Some(content_encoding) = &self.content_encoding {
            builder = builder.header(CONTENT_ENCODING, content_encoding);
        }

        let headers = builder
            .headers_mut()
            // The request building should not have errors at this point, and if it did it would fail in the call to `body()` also.
            .expect("Failed to access headers in http::Request builder- builder has errors.");

        // Static headers from config
        for (header_name, header_value) in self.static_headers.iter() {
            headers.insert(header_name.inner(), header_value.clone());
        }

        // Template headers from the partition key
        for (name, value) in metadata.headers.iter() {
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|e| format!("Invalid header name '{name}': {e}"))?;
            let header_value = HeaderValue::from_bytes(value.as_bytes())
                .map_err(|e| format!("Invalid header value '{value}': {e}"))?;
            headers.insert(header_name, header_value);
        }

        // The request building should not have errors at this point
        let mut request = builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::<crate::Error>::into)?;

        if let Some(auth) = auth {
            auth.apply(&mut request);
        }

        Ok(request)
    }
}
