use super::prelude::error_stage;
use metrics::{counter, gauge};
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct LuaGcTriggered {
    pub used_memory: usize,
}

impl InternalEvent for LuaGcTriggered {
    fn emit_metrics(&self) {
        gauge!("lua_memory_used_bytes", self.used_memory as f64);
    }
}

#[derive(Debug)]
pub struct LuaScriptError {
    pub error: mlua::Error,
}

impl InternalEvent for LuaScriptError {
    fn emit_logs(&self) {
        error!(
            message = "Error in lua script; discarding event.",
            error = ?self.error,
            error_type = "script_failed",
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "script_failed",
            "stage" => error_stage::PROCESSING,
        );
        counter!(
            "component_discarded_events_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "script_failed",
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct LuaBuildError {
    pub error: crate::transforms::lua::v2::BuildError,
}

impl InternalEvent for LuaBuildError {
    fn emit_logs(&self) {
        error!(
            message = "Error in lua script; discarding event.",
            error = ?self.error,
            error_type = "script_failed",
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "script_failed",
            "stage" => error_stage::PROCESSING,
        );
        counter!(
            "component_discarded_events_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "script_failed",
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1);
    }
}
