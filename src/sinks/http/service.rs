//! Service implementation for the `http` sink.
//!
//! As this sink leverages a common HTTP implementation of the `Service` itself,
//! this module only contains the `http` sink specific logic.

use std::io::Write;

use bytes::{BufMut, Bytes, BytesMut};
use codecs::{
    encoding::{Framer, Serializer},
    CharacterDelimitedEncoder,
};
use http::{HeaderName, HeaderValue, Method, Request, Uri};
use indexmap::IndexMap;

use crate::{
    http::Auth,
    sinks::{
        prelude::*,
        util::{http_service::HttpServiceRequestBuilder, Compressor, UriSerde},
    },
};

use super::config::HttpMethod;

#[derive(Debug, Clone)]
pub(super) struct HttpSinkRequestBuilder {
    pub(super) uri: UriSerde,
    pub(super) method: HttpMethod,
    pub(super) auth: Option<Auth>,
    pub(super) headers: IndexMap<HeaderName, HeaderValue>,
    pub(super) payload_prefix: String,
    pub(super) payload_suffix: String,
    pub(super) compression: Compression,
    pub(super) encoder: Encoder<Framer>,
}

impl HttpServiceRequestBuilder for HttpSinkRequestBuilder {
    fn build(&self, mut body: BytesMut) -> Request<Bytes> {
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
