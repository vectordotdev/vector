use super::InternalEvent;
use metrics::counter;
use prost::DecodeError;

define_events_processed_bytes!(VectorEventReceived, "sink", "vector");

define_events_processed_bytes!(VectorEventSent, "sink", "vector", "Events sent.");

#[derive(Debug)]
pub struct VectorProtoDecodeError {
    pub error: DecodeError,
}

impl InternalEvent for VectorProtoDecodeError {
    fn emit_logs(&self) {
        error!(message = "failed to decode protobuf message.", error = %self.error, rate_limit_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!(
            "protobuf_decode_errors", 1,
            "component_kind" => "source",
            "component_type" => "vector",
        );
    }
}
