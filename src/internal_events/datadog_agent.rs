use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DatadogAgentRequestReceived {
    pub byte_size: usize,
}

impl InternalEvent for DatadogAgentRequestReceived {
    fn emit_logs(&self) {
        trace!(message = "Received requests.", byte_size = ?self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("component_received_bytes_total", self.byte_size as u64,);
        counter!("requests_received_total", 1);
    }
}

#[derive(Debug)]
pub struct DatadogAgentMetricDecoded {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for DatadogAgentMetricDecoded {
    fn emit_logs(&self) {
        trace!(message = "Decoded metrics.", count = ?self.count);
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64,
        );
        counter!("datadog_metrics_received_in_total", self.count as u64,);
        counter!("events_in_total", self.count as u64,);
    }
}

#[derive(Debug)]
pub struct DatadogAgentLogDecoded {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for DatadogAgentLogDecoded {
    fn emit_logs(&self) {
        trace!(message = "Decoded logs.", count = ?self.count);
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64,
        );
        counter!("datadog_logs_received_in_total", self.count as u64,);
        counter!("events_in_total", self.count as u64,);
    }
}
