use super::InternalEvent;
use metrics::counter;

pub struct BlackholeEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for BlackholeEventReceived {
    fn emit_metrics(&self) {
        counter!(
            "events_received", 1,
            "component_kind" => "sink",
            "component_type" => "blackhole",
        );
        counter!(
            "bytes_received", self.byte_size as u64,
            "component_kind" => "sink",
            "component_type" => "blackhole",
        );
    }
}
