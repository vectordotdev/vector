use super::InternalEvent;
use metrics::counter;
use serde_json::Error;

#[derive(Debug)]
pub struct DockerEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for DockerEventReceived {
    fn emit_logs(&self) {
        trace!(message = "received one event.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1,
                 "component_kind" => "source",
                 "component_name" => "docker",
        );
        counter!("bytes_processed", self.byte_size as u64,
                 "component_kind" => "source",
                 "component_name" => "docker",
        );
    }
}
