use std::num::NonZeroUsize;

use vector_core::{config, ByteSizeOf, EstimatedJsonEncodedSizeOf};

use vector_common::{
    internal_event::CountByteSize,
    json_size::JsonSize,
    request_metadata::{GetEventCountTags, GroupedCountByteSize, RequestMetadata},
};

use super::request_builder::EncodeResult;

#[derive(Clone, Default)]
pub struct RequestMetadataBuilder {
    event_count: usize,
    events_byte_size: usize,
    events_estimated_json_encoded_byte_size: GroupedCountByteSize,
}

impl RequestMetadataBuilder {
    pub fn from_events<E>(events: &[E]) -> Self
    where
        E: ByteSizeOf + GetEventCountTags + EstimatedJsonEncodedSizeOf,
    {
        let mut size = config::telemetry().create_request_count_byte_size();

        let mut events_byte_size = 0;

        for event in events {
            events_byte_size += event.size_of();
            size.add_event(event, event.estimated_json_encoded_size_of());
        }

        Self {
            event_count: events.len(),
            events_byte_size,
            events_estimated_json_encoded_byte_size: size,
        }
    }

    pub fn from_event<E>(event: &E) -> Self
    where
        E: ByteSizeOf + GetEventCountTags + EstimatedJsonEncodedSizeOf,
    {
        let mut size = config::telemetry().create_request_count_byte_size();
        size.add_event(event, event.estimated_json_encoded_size_of());

        Self {
            event_count: 1,
            events_byte_size: event.size_of(),
            events_estimated_json_encoded_byte_size: size,
        }
    }

    pub fn new(
        event_count: usize,
        events_byte_size: usize,
        events_estimated_json_encoded_byte_size: JsonSize,
    ) -> Self {
        Self {
            event_count,
            events_byte_size,
            events_estimated_json_encoded_byte_size: CountByteSize(
                event_count,
                events_estimated_json_encoded_byte_size,
            )
            .into(),
        }
    }

    pub fn track_event<E>(&mut self, event: E)
    where
        E: ByteSizeOf + GetEventCountTags + EstimatedJsonEncodedSizeOf,
    {
        self.event_count += 1;
        self.events_byte_size += event.size_of();
        let json_size = event.estimated_json_encoded_size_of();
        self.events_estimated_json_encoded_byte_size
            .add_event(&event, json_size);
    }

    pub fn with_request_size(&self, size: NonZeroUsize) -> RequestMetadata {
        let size = size.get();

        RequestMetadata::new(
            self.event_count,
            self.events_byte_size,
            size,
            size,
            self.events_estimated_json_encoded_byte_size.clone(),
        )
    }

    pub fn build<T>(&self, result: &EncodeResult<T>) -> RequestMetadata {
        RequestMetadata::new(
            self.event_count,
            self.events_byte_size,
            result.uncompressed_byte_size,
            result
                .compressed_byte_size
                .unwrap_or(result.uncompressed_byte_size),
            self.events_estimated_json_encoded_byte_size.clone(),
        )
    }
}
