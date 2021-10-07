use crate::sinks::util::RequestBuilder;
use crate::event::Event;
use crate::sinks::elasticsearch::finish_signer;
use rusoto_core::signature::{SignedRequest, SignedRequestPayload};
use rusoto_core::credential::AwsCredentials;
use headers::{HeaderName, HeaderValue};
use http::Uri;

pub struct ElasticsearchRequestBuilder {
    bulk_uri: Uri,
}

impl RequestBuilder<Vec<Event>> for ElasticsearchRequestBuilder {
    type Metadata = (Option<AwsCredentials>,);
    type Events = (Vec<Event>, Option<AwsCredentials>);
    type Payload = http::Request<Vec<u8>>;
    type Request = ();

    fn split_input(&self, input: Events) -> (Self::Metadata, Self::Events) {
        let (events, aws_creds) = input;
        ((aws_creds,), events)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (maybe_credentials,) = metadata;
        let mut builder = Request::post(&self.bulk_uri);

        if let Some(credentials) = maybe_credentials {
            let mut request = self.signed_request("POST", &self.bulk_uri, true);

            request.add_header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression.content_encoding() {
                request.add_header("Content-Encoding", ce);
            }

            for (header, value) in &self.request.headers {
                request.add_header(header, value);
            }

            request.set_payload(Some(events));
            builder = sign_request(&mut request, &credentials, builder);

            // The SignedRequest ends up owning the body, so we have
            // to play games here
            let body = request.payload.take().unwrap();
            match body {
                SignedRequestPayload::Buffer(body) => {
                    builder.body(body.to_vec()).map_err(Into::into)
                }
                _ => unreachable!(),
            }
        } else {
            builder = builder.header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression.content_encoding() {
                builder = builder.header("Content-Encoding", ce);
            }

            for (header, value) in &self.request.headers {
                builder = builder.header(&header[..], &value[..]);
            }

            if let Some(auth) = &self.authorization {
                builder = auth.apply_builder(builder);
            }

            builder.body(events).map_err(Into::into)
        }
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
