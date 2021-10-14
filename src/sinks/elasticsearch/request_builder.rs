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
use crate::sinks::util::encoding::{Encoder, EncodingConfigFixed};
use crate::event::{EventFinalizers, Finalizable};

pub struct ElasticsearchRequestBuilder {
    pub compression: Compression,
    pub encoder: EncodingConfigFixed<ElasticSearchEncoder>,
}

pub struct Metadata {
    finalizers: EventFinalizers,
    batch_size: usize,
}

impl RequestBuilder<Vec<ProcessedEvent>> for ElasticsearchRequestBuilder {
    type Metadata = Metadata;
    type Events = Vec<ProcessedEvent>;
    type Encoder = EncodingConfigFixed<ElasticSearchEncoder>;
    type Payload = Vec<u8>;
    type Request = ElasticSearchRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, mut events: Vec<ProcessedEvent>) -> (Self::Metadata, Self::Events) {
        let metadata = Metadata {
            finalizers: events.take_finalizers(),
            batch_size: events.len(),
        };
        (metadata, events)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Vec<u8>) -> Self::Request {
        ElasticSearchRequest {
            payload,
            finalizers: metadata.finalizers,
            batch_size: metadata.batch_size,
        }
    }
}

