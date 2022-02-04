use metrics::counter;
pub use vector_core::internal_event::EventsReceived;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct BytesReceived {
    pub byte_size: usize,
    pub protocol: &'static str,
}

impl InternalEvent for BytesReceived {
    fn emit_logs(&self) {
        trace!(message = "Bytes received.", byte_size = %self.byte_size, protocol = %self.protocol);
    }

    fn emit_metrics(&self) {
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
    fn emit_logs(&self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint,
        );
    }

    fn emit_metrics(&self) {
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
    fn emit_logs(&self) {
        trace!(
            message = "Bytes sent.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => self.protocol.to_string(),
            "endpoint" => self.endpoint.to_string()
        );
    }
}

#[cfg(feature = "rusoto")]
pub struct AwsBytesSent {
    pub byte_size: usize,
    pub region: rusoto_core::Region,
}

#[cfg(feature = "rusoto")]
impl InternalEvent for AwsBytesSent {
    fn emit_logs(&self) {
        trace!(message = "Bytes sent.", byte_size = %self.byte_size, region = ?self.region);
    }

    fn emit_metrics(&self) {
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => "https",
            "region" => self.region.name().to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct StreamClosedError {
    pub error: crate::source_sender::ClosedError,
    pub count: usize,
}

impl InternalEvent for StreamClosedError {
    fn emit_logs(&self) {
        error!(
            message = "Failed to forward event(s), downstream is closed.",
            error = %self.error,
            error_type = "stream_closed",
            stage = "sending",
            count = %self.count,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "stream_closed",
            "stage" => "sending",
        );
        counter!(
            "component_discarded_events_total", self.count as u64,
            "error" => self.error.to_string(),
            "error_type" => "stream_closed",
            "stage" => "sending",
        );
    }
}
