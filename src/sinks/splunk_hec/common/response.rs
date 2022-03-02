use vector_core::{event::EventStatus, internal_event::EventsSent, stream::DriverResponse};

pub struct HecResponse {
    pub event_status: EventStatus,
    pub events_count: usize,
    pub events_byte_size: usize,
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

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.events_count,
            byte_size: self.events_byte_size,
            output: None,
        }
    }
}
