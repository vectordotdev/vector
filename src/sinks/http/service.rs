//! Service implementation for the `http` sink.

use bytes::Bytes;
use http::{
    header::{CONTENT_ENCODING, CONTENT_TYPE},
    HeaderName, HeaderValue, Method, Request, Uri,
};
use indexmap::IndexMap;

use crate::{
    http::{Auth, BearerTokenState, HttpClient},
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
use async_trait::async_trait;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct HttpSinkRequestBuilder {
    http_client: HttpClient,
    bearer_token_state: BearerTokenState,
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
        http_client: HttpClient,
        bearer_token_state: BearerTokenState,
        uri: UriSerde,
        method: HttpMethod,
        auth: Option<Auth>,
        headers: IndexMap<HeaderName, HeaderValue>,
        content_type: Option<String>,
        content_encoding: Option<String>,
    ) -> Self {
        Self {
            http_client,
            bearer_token_state,
            uri,
            method,
            auth,
            headers,
            content_type,
            content_encoding,
        }
    }
}
#[async_trait]
impl HttpServiceRequestBuilder<()> for HttpSinkRequestBuilder {
    async fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {

        println!("XOXOXOX HttpServiceRequestBuilder::build");

        let method: Method = self.method.into();
        let uri: Uri = self.uri.uri.clone();
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

        if let Some(auth) = &self.auth {
            auth.apply_async(&mut request, self.http_client.clone(), self.bearer_token_state.clone()).await;
        }

        Ok(request)
    }
}
