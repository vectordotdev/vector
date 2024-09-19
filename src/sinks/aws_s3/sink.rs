use std::io;

use bytes::Bytes;
use chrono::{FixedOffset, Utc};
use uuid::Uuid;
use vector_lib::codecs::encoding::Framer;
use vector_lib::event::Finalizable;
use vector_lib::request_metadata::RequestMetadata;

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
            vector_event::{VectorEventLogSendMetadata, generate_count_map},
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
    pub filename_tz_offset: Option<FixedOffset>,
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

        // TODO: There's a good amount of overlapping code for event logs and we will continue to
        // need to update more sinks with this functionality. Might be tricky but might be good to
        // refactor/update the base request builder class to minimize this duplication

        // Create event metadata here as this is where the list of events are available pre-encoding
        // And we want to access this list to process the raw events to see specific field values
        let event_log_metadata = VectorEventLogSendMetadata {
            // Events are not encoded here yet, so byte size is not yet known
            // Setting as 0 here and updating when it is set in build_request()
            bytes: 0,
            events_len: events.len(),
            // Similarly the exact blob isn't determined here yet
            blob: "".to_string(),
            container: self.bucket.clone(),
            count_map: generate_count_map(&events),
        };

        let metadata = S3Metadata {
            partition_key,
            s3_key: s3_key_prefix,
            count: events.len(),
            finalizers,
            event_log_metadata: event_log_metadata,
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
            let formatted_ts = match self.filename_tz_offset {
                Some(offset) => Utc::now()
                    .with_timezone(&offset)
                    .format(self.filename_time_format.as_str()),
                None => Utc::now()
                    .with_timezone(&chrono::Utc)
                    .format(self.filename_time_format.as_str()),
            };

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

        s3metadata.s3_key = format_s3_key(&s3metadata.s3_key, &filename, &extension);

        let body = payload.into_payload();

        // Update some components of the metadata since they've been computed now
        s3metadata.event_log_metadata.bytes = body.len();
        s3metadata.event_log_metadata.blob = s3metadata.s3_key.clone();
        s3metadata.event_log_metadata.emit_sending_event();

        S3Request {
            body: body,
            bucket: self.bucket.clone(),
            metadata: s3metadata,
            request_metadata,
            content_encoding: self.compression.content_encoding(),
            options: s3_options,
        }
    }
}

fn format_s3_key(s3_key: &str, filename: &str, extension: &str) -> String {
    if extension.is_empty() {
        format!("{}{}", s3_key, filename)
    } else {
        format!("{}{}.{}", s3_key, filename, extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_s3_key() {
        assert_eq!(
            "s3_key_filename.txt",
            format_s3_key("s3_key_", "filename", "txt")
        );
        assert_eq!("s3_key_filename", format_s3_key("s3_key_", "filename", ""));
    }
}
