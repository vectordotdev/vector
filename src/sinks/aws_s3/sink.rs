use std::io;

use bytes::Bytes;
use chrono::Utc;
use codecs::encoding::Framer;
use uuid::Uuid;
use vector_common::request_metadata::RequestMetadata;
use vector_core::event::Finalizable;

use crate::{
    codecs::{Encoder, Transformer},
    event::Event,
    sinks::{
        s3_common::{
            config::S3Options,
            partitioner::S3PartitionKey,
            service::{S3Metadata, S3Request},
        },
        util::{
            metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
            RequestBuilder,
        },
    },
};

#[derive(Clone)]
pub struct S3RequestOptions {
    pub bucket: String,
    pub filename_time_format: String,
    pub filename_append_uuid: bool,
    pub filename_extension: Option<String>,
    pub api_options: S3Options,
    pub encoder: (Transformer, Encoder<Framer>),
    pub compression: Compression,
}

impl RequestBuilder<(S3PartitionKey, Vec<Event>)> for S3RequestOptions {
    type Metadata = S3Metadata;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = S3Request;
    type Error = io::Error; // TODO: this is ugly.

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (S3PartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (partition_key, mut events) = input;
        let builder = RequestMetadataBuilder::from_events(&events);

        let finalizers = events.take_finalizers();
        let s3_key_prefix = partition_key.key_prefix.clone();

        let metadata = S3Metadata {
            partition_key,
            s3_key: s3_key_prefix,
            finalizers,
        };

        (metadata, builder, events)
    }

    fn build_request(
        &self,
        mut s3metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let filename = {
            let formatted_ts = Utc::now().format(self.filename_time_format.as_str());

            self.filename_append_uuid
                .then(|| format!("{}-{}", formatted_ts, Uuid::new_v4().hyphenated()))
                .unwrap_or_else(|| formatted_ts.to_string())
        };

        let ssekms_key_id = s3metadata.partition_key.ssekms_key_id.clone();
        let mut s3_options = self.api_options.clone();
        s3_options.ssekms_key_id = ssekms_key_id;

        let extension = self
            .filename_extension
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.compression.extension().into());

        s3metadata.s3_key = format!("{}{}.{}", s3metadata.s3_key, filename, extension);

        S3Request {
            body: payload.into_payload(),
            bucket: self.bucket.clone(),
            metadata: s3metadata,
            request_metadata,
            content_encoding: self.compression.content_encoding(),
            options: s3_options,
        }
    }
}
