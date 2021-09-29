use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct CompoundErrorEvents {
    pub count: usize,
}

impl InternalEvent for CompoundErrorEvents {
    fn emit_metrics(&self) {
        counter!("processing_errors_total", self.count as u64, "error_type" => "transform_failed");
    }
}
