use super::InternalEvent;
use metrics::{counter, gauge};

#[derive(Debug)]
pub struct LuaEventProcessed;

impl InternalEvent for LuaEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub struct LuaGcTriggered {
    pub used_memory: usize,
}

impl InternalEvent for LuaGcTriggered {
    fn emit_metrics(&self) {
        gauge!("memory_used", self.used_memory as f64);
    }
}

#[derive(Debug)]
pub struct LuaScriptError {
    pub error: rlua::Error,
}

impl InternalEvent for LuaScriptError {
    fn emit_logs(&self) {
        error!(message = "Error in lua script; discarding event.", error = %self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1);
    }
}

#[derive(Debug)]
pub struct LuaBuildError {
    pub error: crate::transforms::lua::v2::BuildError,
}

impl InternalEvent for LuaBuildError {
    fn emit_logs(&self) {
        error!(message = "Error in lua script; discarding event.", error = %self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1);
    }
}
