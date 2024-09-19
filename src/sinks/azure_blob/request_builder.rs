use bytes::Bytes;
use chrono::Utc;
use uuid::Uuid;
use vector_lib::codecs::encoding::Framer;
use vector_lib::request_metadata::RequestMetadata;
use vector_lib::EstimatedJsonEncodedSizeOf;

use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, Finalizable},
    sinks::{
        azure_common::config::{AzureBlobMetadata, AzureBlobRequest},
        util::{
            metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
            RequestBuilder,
            vector_event::{VectorEventLogSendMetadata, generate_count_map}
        },
    },
};

#[derive(Clone)]
pub struct AzureBlobRequestOptions {
    pub container_name: String,
    pub blob_time_format: String,
    pub blob_append_uuid: bool,
    pub encoder: (Transformer, Encoder<Framer>),
    pub compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for AzureBlobRequestOptions {
    type Metadata = AzureBlobMetadata;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = AzureBlobRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (String, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();

        // Create event metadata here as this is where the list of events are available pre-encoding
        // And we want to access this list to process the raw events to see specific field values
        let event_log_metadata = VectorEventLogSendMetadata {
            // Events are not encoded here yet, so byte size is not yet known
            // Setting as 0 here and updating when it is set in build_request()
            bytes: 0,
            events_len: events.len(),
            // Similarly the exact blob isn't determined here yet
            blob: "".to_string(),
            container: self.container_name.clone(),
            count_map: generate_count_map(&events),
        };

        let azure_metadata = AzureBlobMetadata {
            partition_key,
            container_name: self.container_name.clone(),
            count: events.len(),
            byte_size: events.estimated_json_encoded_size_of(),
            finalizers,
            event_log_metadata: event_log_metadata,
        };

        let builder = RequestMetadataBuilder::from_events(&events);

        (azure_metadata, builder, events)
    }

    fn build_request(
        &self,
        mut azure_metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let blob_name = {
            let formatted_ts = Utc::now().format(self.blob_time_format.as_str());

            self.blob_append_uuid
                .then(|| format!("{}-{}", formatted_ts, Uuid::new_v4().hyphenated()))
                .unwrap_or_else(|| formatted_ts.to_string())
        };

        let extension = self.compression.extension();
        azure_metadata.partition_key = format!(
            "{}{}.{}",
            azure_metadata.partition_key, blob_name, extension
        );

        let blob_data = payload.into_payload();

        // Update some components of the metadata since they've been computed now
        azure_metadata.event_log_metadata.bytes = blob_data.len();
        azure_metadata.event_log_metadata.blob = azure_metadata.partition_key.clone();
        azure_metadata.event_log_metadata.emit_sending_event();

        AzureBlobRequest {
            blob_data,
            content_encoding: self.compression.content_encoding(),
            content_type: self.encoder.1.content_type(),
            metadata: azure_metadata,
            request_metadata,
        }
    }
}
