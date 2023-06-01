use vector_common::json_size::JsonSize;
use vector_core::internal_event::CountByteSize;
use vector_core::{event::EventStatus, stream::DriverResponse};

pub struct HecResponse {
    pub event_status: EventStatus,
    pub events_count: usize,
    pub events_byte_size: JsonSize,
}

impl AsRef<EventStatus> for HecResponse {
    fn as_ref(&self) -> &EventStatus {
        &self.event_status
    }
}

impl DriverResponse for HecResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(self.events_count, self.events_byte_size)
    }
}
