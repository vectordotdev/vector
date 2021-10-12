use crate::sinks::util::{RequestBuilder, Compression};
use crate::event::Event;
use crate::sinks::elasticsearch::{finish_signer, BulkAction};
use rusoto_core::signature::{SignedRequest, SignedRequestPayload};
use rusoto_core::credential::AwsCredentials;
use headers::{HeaderName, HeaderValue};
use http::Uri;
use crate::sinks::elasticsearch::encoder::ElasticSearchEncoder;
use vector_core::ByteSizeOf;
use crate::sinks::elasticsearch::service::ElasticSearchRequest;

pub struct ElasticsearchRequestBuilder {
    bulk_uri: Uri,
}

pub struct ProcessedEvent {
    index: String,
    bulk_action: BulkAction,
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

    // fn encode_events(&self, events: Self::Events) -> Result<Self::Payload, Self::Error> {
    //     let mut compressor = Compressor::from(self.compression());
    //     let _ = self.encoder().encode_input(events, &mut compressor)?;
    //
    //     let payload = compressor.into_inner().into();
    //     Ok(payload)
    // }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        todo!();
        // let (maybe_credentials,) = metadata;
        // let mut builder = Request::post(&self.bulk_uri);
        //
        // if let Some(credentials) = maybe_credentials {
        //     let mut request = self.signed_request("POST", &self.bulk_uri, true);
        //
        //     request.add_header("Content-Type", "application/x-ndjson");
        //
        //     if let Some(ce) = self.compression.content_encoding() {
        //         request.add_header("Content-Encoding", ce);
        //     }
        //
        //     for (header, value) in &self.request.headers {
        //         request.add_header(header, value);
        //     }
        //
        //     request.set_payload(Some(events));
        //     builder = sign_request(&mut request, &credentials, builder);
        //
        //     // The SignedRequest ends up owning the body, so we have
        //     // to play games here
        //     let body = request.payload.take().unwrap();
        //     match body {
        //         SignedRequestPayload::Buffer(body) => {
        //             builder.body(body.to_vec()).map_err(Into::into)
        //         }
        //         _ => unreachable!(),
        //     }
        // } else {
        //     builder = builder.header("Content-Type", "application/x-ndjson");
        //
        //     if let Some(ce) = self.compression.content_encoding() {
        //         builder = builder.header("Content-Encoding", ce);
        //     }
        //
        //     for (header, value) in &self.request.headers {
        //         builder = builder.header(&header[..], &value[..]);
        //     }
        //
        //     if let Some(auth) = &self.authorization {
        //         builder = auth.apply_builder(builder);
        //     }
        //
        //     builder.body(events).map_err(Into::into)
        // }
        // http::Request<Vec<u8>>
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
