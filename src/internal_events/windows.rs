use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct WindowsServiceStart<'a> {
    pub already_started: bool,
    pub name: &'a str,
}

impl<'a> InternalEvent for WindowsServiceStart<'a> {
    fn emit(self) {
        info!(
            already_started = %self.already_started,
            name = self.name,
            "Started Windows Service.",
        );
        counter!("windows_service_start_total", 1,
            "already_started" => self.already_started.to_string(),
        );
    }
}

#[derive(Debug)]
pub struct WindowsServiceStop<'a> {
    pub already_stopped: bool,
    pub name: &'a str,
}

impl<'a> InternalEvent for WindowsServiceStop<'a> {
    fn emit(self) {
        info!(
            already_stopped = %self.already_stopped,
            name = ?self.name,
            "Stopped Windows Service.",
        );
        counter!("windows_service_stop_total", 1,
            "already_stopped" => self.already_stopped.to_string(),
        );
    }
}

#[derive(Debug)]
pub struct WindowsServiceRestart<'a> {
    pub name: &'a str,
}

impl<'a> InternalEvent for WindowsServiceRestart<'a> {
    fn emit(self) {
        info!(
            name = ?self.name,
            "Restarted Windows Service."
        );
        counter!("windows_service_restart_total", 1)
    }
}

#[derive(Debug)]
pub struct WindowsServiceInstall<'a> {
    pub name: &'a str,
}

impl<'a> InternalEvent for WindowsServiceInstall<'a> {
    fn emit(self) {
        info!(
            name = ?self.name,
            "Installed Windows Service.",
        );
        counter!("windows_service_install_total", 1,);
    }
}

#[derive(Debug)]
pub struct WindowsServiceUninstall<'a> {
    pub name: &'a str,
}

impl<'a> InternalEvent for WindowsServiceUninstall<'a> {
    fn emit(self) {
        info!(
            name = ?self.name,
            "Uninstalled Windows Service.",
        );
        counter!("windows_service_uninstall_total", 1,);
    }
}

#[derive(Debug)]
pub struct WindowsServiceDoesNotExistError<'a> {
    pub name: &'a str,
}

impl<'a> InternalEvent for WindowsServiceDoesNotExistError<'a> {
    fn emit(self) {
        error!(
            message = "Windows service does not exist. Maybe it needs to be installed.",
            name = self.name,
            error_code = "service_missing",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "service_missing",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
    }
}
