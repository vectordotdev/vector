use super::InternalEvent;
use metrics::counter;
use prost::DecodeError;

pub struct VectorEventSent {
    pub byte_size: usize,
}

impl InternalEvent for VectorEventSent {
    fn emit_metrics(&self) {
        counter!(
            "events_sent", 1,
            "component_kind" => "sink",
            "component_type" => "vector",
        );
        counter!(
            "bytes_sent", self.byte_size as u64,
            "component_kind" => "sink",
            "component_type" => "vector",
        );
    }
}

pub struct VectorEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for VectorEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received one event.",);
    }

    fn emit_metrics(&self) {
        counter!(
            "events_received", 1,
            "component_kind" => "sink",
            "component_type" => "vector",
        );
        counter!(
            "bytes_received", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "vector",
        );
    }
}

pub struct VectorProtoParseError {
    pub error: DecodeError,
}

impl InternalEvent for VectorProtoParseError {
    fn emit_logs(&self) {
        error!(message = "failed to parse protobuf message", %self.error);
    }

    fn emit_metrics(&self) {
        counter!(
            "protobuf_parse_errors", 1,
            "component_kind" => "source",
            "component_type" => "vector",
        );
    }
}
