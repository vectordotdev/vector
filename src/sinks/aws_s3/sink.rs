use bytes::Bytes;
use chrono::Utc;
use uuid::Uuid;
use vector_common::{finalization::Finalizable, request_metadata::RequestMetadata};
use vector_core::event::Event;

use crate::sinks::{
    s3_common::{
        config::S3Options,
        partitioner::S3PartitionKey,
        service::{S3Metadata, S3Request},
        sink::RequestBuilder,
    },
    util::Compression,
};

#[derive(Clone)]
pub struct S3RequestBuilder {
    pub bucket: String,
    pub filename_time_format: String,
    pub filename_append_uuid: bool,
    pub filename_extension: Option<String>,
    pub api_options: S3Options,
    pub compression: Compression,
}

impl RequestBuilder for S3RequestBuilder {
    type Request = S3Request;
    type Metadata = S3Metadata;
    type PartitionKey = S3PartitionKey;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn build_metadata(
        &self,
        partition_key: Self::PartitionKey,
        mut events: Vec<Event>,
    ) -> S3Metadata {
        // TODO: Finalizers aren't really sink-specific, so can we handle them in a common place
        // instead? Do we need this phase if so? Very little else is happening here.
        let finalizers = events.take_finalizers();
        let s3_key_prefix = partition_key.key_prefix.clone();

        S3Metadata {
            partition_key,
            s3_key: s3_key_prefix,
            finalizers,
        }
    }

    fn build_request(
        &self,
        mut metadata: Self::Metadata,
        payload: Bytes,
        // TODO: This is just passed through, so try moving it to a common wrapper struct/service so
        // individual sinks don't need to deal with it at all.
        request_metadata: RequestMetadata,
    ) -> Self::Request {
        let filename = {
            let formatted_ts = Utc::now().format(self.filename_time_format.as_str());

            self.filename_append_uuid
                .then(|| format!("{}-{}", formatted_ts, Uuid::new_v4().hyphenated()))
                .unwrap_or_else(|| formatted_ts.to_string())
        };

        let ssekms_key_id = metadata.partition_key.ssekms_key_id.clone();
        let mut s3_options = self.api_options.clone();
        s3_options.ssekms_key_id = ssekms_key_id;

        let extension = self
            .filename_extension
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.compression.extension().into());

        metadata.s3_key = format!("{}{}.{}", metadata.s3_key, filename, extension);

        S3Request {
            body: payload,
            bucket: self.bucket.clone(),
            metadata,
            request_metadata,
            content_encoding: self.compression.content_encoding(),
            options: s3_options,
        }
    }
}
