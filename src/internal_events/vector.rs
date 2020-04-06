use super::InternalEvent;
use metrics::counter;
use prost::DecodeError;

#[derive(Debug)]
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

#[derive(Debug)]
pub struct VectorEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for VectorEventReceived {
    fn emit_logs(&self) {
        trace!(message = "received one event.",);
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

#[derive(Debug)]
pub struct VectorProtoDecodeError {
    pub error: DecodeError,
}

impl InternalEvent for VectorProtoDecodeError {
    fn emit_logs(&self) {
        error!(message = "failed to decode protobuf message", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!(
            "protobuf_decode_errors", 1,
            "component_kind" => "source",
            "component_type" => "vector",
        );
    }
}
