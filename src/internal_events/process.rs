use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};
use crate::{built_info, config};

#[derive(Debug)]
pub struct VectorStarted;

impl InternalEvent for VectorStarted {
    fn emit(self) {
        info!(
            target: "vector",
            message = "Vector has started.",
            debug = built_info::DEBUG,
            version = built_info::PKG_VERSION,
            arch = built_info::TARGET_ARCH,
            build_id = built_info::VECTOR_BUILD_DESC.unwrap_or("none"),
        );
        counter!("started_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorReloaded<'a> {
    pub config_paths: &'a [config::ConfigPath],
}

impl InternalEvent for VectorReloaded<'_> {
    fn emit(self) {
        info!(
            target: "vector",
            message = "Vector has reloaded.",
            path = ?self.config_paths
        );
        counter!("reloaded_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorStopped;

impl InternalEvent for VectorStopped {
    fn emit(self) {
        info!(
            target: "vector",
            message = "Vector has stopped."
        );
        counter!("stopped_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorQuit;

impl InternalEvent for VectorQuit {
    fn emit(self) {
        info!(
            target: "vector",
            message = "Vector has quit."
        );
        counter!("quit_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorReloadError;

impl InternalEvent for VectorReloadError {
    fn emit(self) {
        error!(
            message = "Reload was not successful.",
            error_code = "reload",
            error_type = error_type::CONFIGURATION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "reload",
            "error_type" => error_type::CONFIGURATION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("reload_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorConfigLoadError;

impl InternalEvent for VectorConfigLoadError {
    fn emit(self) {
        error!(
            message = "Failed to load config files, reload aborted.",
            error_code = "config_load",
            error_type = error_type::CONFIGURATION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "config_load",
            "error_type" => error_type::CONFIGURATION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("config_load_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorRecoveryError;

impl InternalEvent for VectorRecoveryError {
    fn emit(self) {
        error!(
            message = "Vector has failed to recover from a failed reload.",
            error_code = "recovery",
            error_type = error_type::CONFIGURATION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "recovery",
            "error_type" => error_type::CONFIGURATION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("recover_errors_total", 1);
    }
}
