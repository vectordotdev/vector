use bytes::Bytes;
use http::Response;
use vector_core::event::EventStatus;

pub struct HecResponse {
    pub http_response: Response<Bytes>,
    pub event_status: EventStatus,
}

impl AsRef<EventStatus> for HecResponse {
    fn as_ref(&self) -> &EventStatus {
        &self.event_status
    }
}
