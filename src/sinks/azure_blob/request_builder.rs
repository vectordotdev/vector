use bytes::Bytes;
use chrono::Utc;
use uuid::Uuid;
use vector_core::ByteSizeOf;

use crate::{
    event::{Event, Finalizable},
    sinks::{
        azure_common::config::{AzureBlobMetadata, AzureBlobRequest},
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            Compression, RequestBuilder,
        },
    },
};

#[derive(Clone)]
pub struct AzureBlobRequestOptions {
    pub container_name: String,
    pub blob_time_format: String,
    pub blob_append_uuid: bool,
    pub encoding: EncodingConfig<StandardEncodings>,
    pub compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for AzureBlobRequestOptions {
    type Metadata = AzureBlobMetadata;
    type Events = Vec<Event>;
    type Encoder = EncodingConfig<StandardEncodings>;
    type Payload = Bytes;
    type Request = AzureBlobRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(&self, input: (String, Vec<Event>)) -> (Self::Metadata, Self::Events) {
        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();
        let metadata = AzureBlobMetadata {
            partition_key,
            count: events.len(),
            byte_size: events.size_of(),
            finalizers,
        };

        (metadata, events)
    }

    fn build_request(&self, mut metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let blob_name = {
            let formatted_ts = Utc::now().format(self.blob_time_format.as_str());

            self.blob_append_uuid
                .then(|| format!("{}-{}", formatted_ts, Uuid::new_v4().to_hyphenated()))
                .unwrap_or_else(|| formatted_ts.to_string())
        };

        let extension = self.compression.extension();
        metadata.partition_key = format!("{}{}.{}", metadata.partition_key, blob_name, extension);

        debug!(
            message = "Sending events.",
            bytes = ?payload.len(),
            events_len = ?metadata.count,
            blob = ?metadata.partition_key,
            container = ?self.container_name,
        );

        AzureBlobRequest {
            blob_data: payload,
            content_encoding: self.compression.content_encoding(),
            content_type: self.compression.content_type(),
            metadata,
        }
    }
}

impl Compression {
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::None => "text/plain",
            Self::Gzip(_) => "application/gzip",
        }
    }
}
