use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct RemapEventProcessed;

impl InternalEvent for RemapEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "remap",
        );
    }
}

#[derive(Debug)]
pub struct RemapFailedMapping {
    pub error: String,
}

impl InternalEvent for RemapFailedMapping {
    fn emit_logs(&self) {
        warn!(
            message = "Mapping failed with event",
            %self.error,
            rate_limit_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_error", 1,
            "component_kind" => "transform",
            "component_type" => "remap",
            "error_type" => "failed_mapping",
        );
    }
}
