use super::InternalEvent;
use metrics::counter;
use std::path::PathBuf;

#[derive(Debug)]
pub struct VectorStarted;

impl InternalEvent for VectorStarted {
    fn emit_logs(&self) {
        info!(
            target: "vector",
            message = "Vector has started.",
            version = built_info::PKG_VERSION,
            git_version = built_info::GIT_VERSION.unwrap_or(""),
            released = built_info::BUILT_TIME_UTC,
            arch = built_info::CFG_TARGET_ARCH
        );
    }

    fn emit_metrics(&self) {
        counter!("vector_started_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorReloaded<'a> {
    pub config_paths: &'a [PathBuf],
}

impl InternalEvent for VectorReloaded<'_> {
    fn emit_logs(&self) {
        info!(
            target: "vector",
            message = "Vector has reloaded.",
            path = ?self.config_paths
        );
    }

    fn emit_metrics(&self) {
        counter!("vector_reloaded_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorStopped;

impl InternalEvent for VectorStopped {
    fn emit_logs(&self) {
        info!(
            target: "vector",
            message = "Vector has stopped."
        );
    }

    fn emit_metrics(&self) {
        counter!("vector_stopped_total", 1);
    }
}

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
