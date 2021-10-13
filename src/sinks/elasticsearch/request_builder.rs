use crate::sinks::util::{RequestBuilder, Compression};


use rusoto_core::signature::{SignedRequest, SignedRequestPayload};
use rusoto_core::credential::AwsCredentials;
use headers::{HeaderName, HeaderValue};
use http::Uri;
use crate::sinks::elasticsearch::encoder::{ElasticSearchEncoder, ProcessedEvent};
use vector_core::ByteSizeOf;
use crate::sinks::elasticsearch::service::ElasticSearchRequest;

use crate::sinks::util::http::RequestConfig;
use crate::http::Auth;
use http::Request;
use std::collections::HashMap;
use rusoto_core::Region;

pub struct ElasticsearchRequestBuilder {
    pub bulk_uri: Uri,
    pub http_request_config: RequestConfig,
    pub http_auth: Option<Auth>,
    pub query_params: HashMap<String, String>,
    pub region: Region,
}



impl ByteSizeOf for ProcessedEvent {
    fn allocated_bytes(&self) -> usize {
        todo!()
    }
}

pub struct Input {
    pub aws_credentials: Option<AwsCredentials>,
    pub events: Vec<ProcessedEvent>
}

pub struct Metadata {
    aws_credentials: Option<AwsCredentials>
}

impl RequestBuilder<Input> for ElasticsearchRequestBuilder {
    type Metadata = Metadata;
    type Events = Vec<ProcessedEvent>;
    type Encoder = ElasticSearchEncoder;
    type Payload = Vec<u8>;
    type Request = ElasticSearchRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        todo!()
    }

    fn encoder(&self) -> &Self::Encoder {
        todo!()
    }

    fn split_input(&self, input: Input) -> (Self::Metadata, Self::Events) {
        let metadata = Metadata {
            aws_credentials: input.aws_credentials
        };
        (metadata, input.events)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let mut builder = Request::post(&self.bulk_uri);

        let _http_req = if let Some(aws_credentials) = metadata.aws_credentials {
            let mut request = self.create_signed_request("POST", &self.bulk_uri, true);

            request.add_header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression().content_encoding() {
                request.add_header("Content-Encoding", ce);
            }

            for (header, value) in &self.http_request_config.headers {
                request.add_header(header, value);
            }

            request.set_payload(Some(payload));
            builder = sign_request(&mut request, &aws_credentials, builder);

            // The SignedRequest ends up owning the body, so we have
            // to play games here
            let body = request.payload.take().unwrap();
            match body {
                SignedRequestPayload::Buffer(body) => {
                    builder.body(body.to_vec()).expect("Invalid http request value used")
                }
                _ => unreachable!(),
            }
        } else {
            builder = builder.header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression().content_encoding() {
                builder = builder.header("Content-Encoding", ce);
            }

            for (header, value) in &self.http_request_config.headers {
                builder = builder.header(&header[..], &value[..]);
            }

            if let Some(auth) = &self.http_auth {
                builder = auth.apply_builder(builder);
            }

            builder.body(payload).expect("Invalid http request value used")
        };
        // http::Request<Vec<u8>>
        todo!()
    }
}

impl ElasticsearchRequestBuilder {

    fn create_signed_request(&self, method: &str, uri: &Uri, use_params: bool) -> SignedRequest {
        let mut request = SignedRequest::new(method, "es", &self.region, uri.path());
        request.set_hostname(uri.host().map(|host| host.into()));
        if use_params {
            for (key, value) in &self.query_params {
                request.add_param(key, value);
            }
        }
        request
    }
}


fn sign_request(
    request: &mut SignedRequest,
    credentials: &AwsCredentials,
    mut builder: http::request::Builder,
) -> http::request::Builder {
    request.sign(&credentials);

    for (name, values) in request.headers() {
        let header_name = name
            .parse::<HeaderName>()
            .expect("Could not parse header name.");
        for value in values {
            let header_value =
                HeaderValue::from_bytes(value).expect("Could not parse header value.");
            builder = builder.header(&header_name, header_value);
        }
    }
    builder
}
