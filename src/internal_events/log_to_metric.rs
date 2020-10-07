use super::InternalEvent;
use metrics::counter;
use std::num::ParseFloatError;
use string_cache::DefaultAtom;

pub(crate) struct LogToMetricEventProcessed;

impl InternalEvent for LogToMetricEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "log_to_metric",
        );
    }
}

pub(crate) struct LogToMetricFieldNotFound {
    pub field: DefaultAtom,
}

impl InternalEvent for LogToMetricFieldNotFound {
    fn emit_logs(&self) {
        warn!(
            message = "Field not found.",
            missing_field = %self.field,
            rate_limit_sec = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
                 "component_kind" => "transform",
                 "component_type" => "log_to_metric",
                 "error_type" => "field_not_found",
        );
    }
}

pub(crate) struct LogToMetricParseFloatError {
    pub field: DefaultAtom,
    pub error: ParseFloatError,
}

impl InternalEvent for LogToMetricParseFloatError {
    fn emit_logs(&self) {
        warn!(
            message = "Failed to parse field as float.",
            field = %self.field,
            error = %self.error,
            rate_limit_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
                 "component_kind" => "transform",
                 "component_type" => "log_to_metric",
                 "error_type" => "parse_error",
        );
    }
}

pub(crate) struct LogToMetricRenderError {
    pub error: String,
}

impl InternalEvent for LogToMetricRenderError {
    fn emit_logs(&self) {
        warn!(message = "Unable to render.", error = %self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
                 "component_kind" => "transform",
                 "component_type" => "log_to_metric",
                 "error_type" => "render_error",
        );
    }
}

pub(crate) struct LogToMetricTemplateParseError {
    pub error: crate::template::TemplateError,
}

impl InternalEvent for LogToMetricTemplateParseError {
    fn emit_logs(&self) {
        warn!(message = "Failed to parse template.", error = %self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
                 "component_kind" => "transform",
                 "component_type" => "log_to_metric",
                 "error_type" => "template_error",
        );
    }
}
