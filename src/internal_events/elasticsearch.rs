use super::InternalEvent;
use crate::template::TemplateRenderingError;
use metrics::counter;

#[derive(Debug)]
pub struct ElasticSearchEventEncoded {
    pub byte_size: usize,
    pub index: String,
}

impl InternalEvent for ElasticSearchEventEncoded {
    fn emit_logs(&self) {
        trace!(message = "Inserting event.", index = %self.index);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct ElasticSearchTemplateRenderingError {
    pub error: TemplateRenderingError,
}

impl InternalEvent for ElasticSearchTemplateRenderingError {
    fn emit_logs(&self) {
        warn!(
            message = "Failed to render template; dropping event.",
            error = %self.error,
            internal_log_rate_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!("template_rendering_errors_total", 1);
    }
}
