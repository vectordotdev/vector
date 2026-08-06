use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};

#[derive(Debug, NamedInternalEvent)]
pub struct WindowsServiceStart<'a> {
    pub already_started: bool,
    pub name: &'a str,
}

impl InternalEvent for WindowsServiceStart<'_> {
    fn emit(self) {
        info!(
            already_started = %self.already_started,
            name = self.name,
            "Started Windows Service.",
        );
        counter!(
            CounterName::WindowsServiceStartTotal,
            "already_started" => self.already_started.to_string(),
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WindowsServiceStop<'a> {
    pub already_stopped: bool,
    pub name: &'a str,
}

impl InternalEvent for WindowsServiceStop<'_> {
    fn emit(self) {
        info!(
            already_stopped = %self.already_stopped,
            name = ?self.name,
            "Stopped Windows Service.",
        );
        counter!(
            CounterName::WindowsServiceStopTotal,
            "already_stopped" => self.already_stopped.to_string(),
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WindowsServiceRestart<'a> {
    pub name: &'a str,
}

impl InternalEvent for WindowsServiceRestart<'_> {
    fn emit(self) {
        info!(
            name = ?self.name,
            "Restarted Windows Service."
        );
        counter!(CounterName::WindowsServiceRestartTotal).increment(1)
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WindowsServiceInstall<'a> {
    pub name: &'a str,
}

impl InternalEvent for WindowsServiceInstall<'_> {
    fn emit(self) {
        info!(
            name = ?self.name,
            "Installed Windows Service.",
        );
        counter!(CounterName::WindowsServiceInstallTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WindowsServiceUninstall<'a> {
    pub name: &'a str,
}

impl InternalEvent for WindowsServiceUninstall<'_> {
    fn emit(self) {
        info!(
            name = ?self.name,
            "Uninstalled Windows Service.",
        );
        counter!(CounterName::WindowsServiceUninstallTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WindowsServiceDoesNotExistError<'a> {
    pub name: &'a str,
}

impl InternalEvent for WindowsServiceDoesNotExistError<'_> {
    fn emit(self) {
        error!(
            message = "Windows service does not exist. Maybe it needs to be installed.",
            name = self.name,
            error_code = "service_missing",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "service_missing",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
