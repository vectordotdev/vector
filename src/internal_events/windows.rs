use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct WindowsServiceStart {
    pub already_started: bool,
}

impl InternalEvent for WindowsServiceStart {
    fn emit_logs(&self) {
        info!(
            already_started = %self.already_started,
            "Started Windows Service.",
        );
    }

    fn emit_metrics(&self) {
        counter!("windows_service_start", 1,
            "already_started" => self.already_started.to_string(),
        );
    }
}

#[derive(Debug)]
pub struct WindowsServiceStop {
    pub already_stopped: bool,
}

impl InternalEvent for WindowsServiceStop {
    fn emit_logs(&self) {
        info!(
            already_stopped = %self.already_stopped,
            "Stopped Windows Service.",
        );
    }

    fn emit_metrics(&self) {
        counter!("windows_service_stop", 1,
            "already_stopped" => self.already_stopped.to_string(),
        );
    }
}

#[derive(Debug)]
pub struct WindowsServiceInstall;

impl InternalEvent for WindowsServiceInstall {
    fn emit_logs(&self) {
        info!("Installed Windows Service.");
    }

    fn emit_metrics(&self) {
        counter!("windows_service_install", 1);
    }
}

#[derive(Debug)]
pub struct WindowsServiceUninstall;

impl InternalEvent for WindowsServiceUninstall {
    fn emit_logs(&self) {
        info!("Uninstalled Windows Service.");
    }

    fn emit_metrics(&self) {
        counter!("windows_service_uninstall", 1);
    }
}

#[derive(Debug)]
pub struct WindowsServiceDoesNotExist;

impl InternalEvent for WindowsServiceDoesNotExist {
    fn emit_logs(&self) {
        error!("Windows service does not exist. Maybe it needs to be installed?");
    }

    fn emit_metrics(&self) {
        counter!("windows_service_does_not_exist", 1);
    }
}