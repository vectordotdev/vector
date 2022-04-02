use super::prelude::{error_stage, error_type};
use metrics::counter;
pub use vector_core::internal_event::EventsReceived;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct BytesReceived {
    pub byte_size: usize,
    pub protocol: &'static str,
}

impl InternalEvent for BytesReceived {
    fn emit(self) {
        trace!(message = "Bytes received.", byte_size = %self.byte_size, protocol = %self.protocol);
        counter!("component_received_bytes_total", self.byte_size as u64, "protocol" => self.protocol);
    }
}

#[derive(Debug)]
pub struct HttpClientBytesReceived<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
    pub endpoint: &'a str,
}

impl InternalEvent for HttpClientBytesReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint,
        );
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => self.protocol.to_owned(),
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct EndpointBytesSent<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for EndpointBytesSent<'a> {
    fn emit(self) {
        trace!(
            message = "Bytes sent.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint
        );
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => self.protocol.to_string(),
            "endpoint" => self.endpoint.to_string()
        );
    }
}

#[cfg(feature = "aws-core")]
pub struct AwsBytesSent {
    pub byte_size: usize,
    pub region: Option<aws_types::region::Region>,
}

#[cfg(feature = "aws-core")]
impl InternalEvent for AwsBytesSent {
    fn emit(self) {
        trace!(
            message = "Bytes sent.",
            protocol = "https",
            byte_size = %self.byte_size,
            region = ?self.region,
        );
        let region = self
            .region
            .as_ref()
            .map(|r| r.as_ref().to_string())
            .unwrap_or_default();
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => "https",
            "region" => region,
        );
    }
}

const STREAM_CLOSED: &str = "stream_closed";

#[derive(Debug)]
pub struct StreamClosedError {
    pub error: crate::source_sender::ClosedError,
    pub count: usize,
}

impl InternalEvent for StreamClosedError {
    fn emit(self) {
        error!(
            message = "Failed to forward event(s), downstream is closed.",
            error_code = STREAM_CLOSED,
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            count = %self.count,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => STREAM_CLOSED,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
        counter!(
            "component_discarded_events_total", self.count as u64,
            "error_code" => STREAM_CLOSED,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
    }
}
