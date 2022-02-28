use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::{built_info, config};

#[derive(Debug)]
pub struct VectorStarted;

impl InternalEvent for VectorStarted {
    fn emit_logs(&self) {
        info!(
            target: "vector",
            message = "Vector has started.",
            debug = built_info::DEBUG,
            version = built_info::PKG_VERSION,
            arch = built_info::TARGET_ARCH,
            build_id = built_info::VECTOR_BUILD_DESC.unwrap_or("none"),
        );
    }

    fn emit_metrics(&self) {
        counter!("started_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorReloaded<'a> {
    pub config_paths: &'a [config::ConfigPath],
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
        counter!("reloaded_total", 1);
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
        counter!("stopped_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorQuit;

impl InternalEvent for VectorQuit {
    fn emit_logs(&self) {
        info!(
            target: "vector",
            message = "Vector has quit."
        );
    }

    fn emit_metrics(&self) {
        counter!("quit_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorReloadFailed;

impl InternalEvent for VectorReloadFailed {
    fn emit_logs(&self) {
        error!(
            target: "vector",
            message = "Reload was not successful."
        );
    }

    fn emit_metrics(&self) {
        counter!("reload_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorConfigLoadFailed;

impl InternalEvent for VectorConfigLoadFailed {
    fn emit_logs(&self) {
        error!(
            target: "vector",
            message = "Failed to load config files, reload aborted."
        );
    }

    fn emit_metrics(&self) {
        counter!("config_load_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorRecoveryFailed;

impl InternalEvent for VectorRecoveryFailed {
    fn emit_logs(&self) {
        error!(
            target: "vector",
            message = "Vector has failed to recover from a failed reload."
        );
    }

    fn emit_metrics(&self) {
        counter!("recover_errors_total", 1);
    }
}
