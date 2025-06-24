//! Service implementation for the `http` sink.

use std::str::FromStr;

use bytes::Bytes;
use http::{
    header::{CONTENT_ENCODING, CONTENT_TYPE},
    HeaderName, HeaderValue, Method, Request,
};
use indexmap::IndexMap;

use crate::{
    http::{Auth, MaybeAuth},
    sinks::{
        util::{
            http::{HttpRequest, HttpServiceRequestBuilder},
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
    headers: IndexMap<HeaderName, HeaderValue>,
    content_type: Option<String>,
    content_encoding: Option<String>,
}

impl HttpSinkRequestBuilder {
    /// Creates a new `HttpSinkRequestBuilder`
    pub(super) const fn new(
        method: HttpMethod,
        auth: Option<Auth>,
        headers: IndexMap<HeaderName, HeaderValue>,
        content_type: Option<String>,
        content_encoding: Option<String>,
    ) -> Self {
        Self {
            method,
            auth,
            headers,
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
        // TODO: should we emit some other error here? How is this error handled?
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

        for (header, value) in self.headers.iter() {
            headers.insert(header, value.clone());
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
