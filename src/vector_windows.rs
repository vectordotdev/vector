#![allow(missing_docs)]
use std::{ffi::OsString, time::Duration};

use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::ServiceControlHandlerResult,
    service_dispatcher, Result,
};

use crate::{app::Application, signal::SignalTo};

const SERVICE_NAME: &str = "vector";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

const NO_ERROR: u32 = 0;
const ERROR: u32 = 121;

pub mod service_control {
    use std::{ffi::OsString, fmt, fmt::Formatter, time::Duration};

    use snafu::ResultExt;
    use windows_service::{
        service::{
            ServiceAccess, ServiceErrorControl, ServiceExitCode, ServiceInfo, ServiceStartType,
            ServiceState, ServiceStatus,
        },
        service_manager::{ServiceManager, ServiceManagerAccess},
        Result,
    };

    use crate::{
        internal_events::{
            WindowsServiceDoesNotExistError, WindowsServiceInstall, WindowsServiceRestart,
            WindowsServiceStart, WindowsServiceStop, WindowsServiceUninstall,
        },
        vector_windows::{NO_ERROR, SERVICE_TYPE},
    };

    struct ErrorDisplay<'a> {
        error: &'a windows_service::Error,
    }

    impl fmt::Display for ErrorDisplay<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            if let windows_service::Error::Winapi(ref win_error) = &self.error {
                write!(f, "{}", win_error)
            } else {
                write!(f, "{}", &self.error)
            }
        }
    }

    const fn error_display(error: &windows_service::Error) -> ErrorDisplay {
        ErrorDisplay { error }
    }

    #[derive(Debug, snafu::Snafu)]
    pub enum Error {
        #[snafu(display("{}", error_display(source)))]
        Service {
            #[snafu(source)]
            source: windows_service::Error,
        },
        #[snafu(display(
            "Timeout occurred after {:?} while waiting for state to become {:?}, but was {:?}",
            timeout,
            expected_state,
            state
        ))]
        PollTimeout {
            state: ServiceState,
            expected_state: ServiceState,
            timeout: Duration,
        },
    }

    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum ControlAction {
        Install,
        Uninstall,
        Start,
        Stop,
        Restart { stop_timeout: Duration },
    }

    #[derive(Debug, Clone, PartialEq)]
    enum PollStatus {
        NoTimeout(ServiceStatus),
        Timeout(ServiceStatus),
    }

    impl fmt::Display for ControlAction {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    pub struct ServiceDefinition {
        pub name: OsString,
        pub display_name: OsString,
        pub description: OsString,

        pub executable_path: std::path::PathBuf,
        pub launch_arguments: Vec<OsString>,
    }

    impl std::str::FromStr for ControlAction {
        type Err = String;

        fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
            match s {
                "install" => Ok(ControlAction::Install),
                "uninstall" => Ok(ControlAction::Uninstall),
                "start" => Ok(ControlAction::Start),
                "stop" => Ok(ControlAction::Stop),
                _ => Err(format!("invalid option {} for ControlAction", s)),
            }
        }
    }

    pub fn control(service_def: &ServiceDefinition, action: ControlAction) -> crate::Result<()> {
        match action {
            ControlAction::Start => start_service(service_def),
            ControlAction::Stop => stop_service(service_def),
            ControlAction::Restart { stop_timeout } => restart_service(service_def, stop_timeout),
            ControlAction::Install => install_service(service_def),
            ControlAction::Uninstall => uninstall_service(service_def),
        }
    }

    fn start_service(service_def: &ServiceDefinition) -> crate::Result<()> {
        let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::START;
        let service = open_service(service_def, service_access)?;
        let service_status = service.query_status().context(ServiceSnafu)?;

        if service_status.current_state != ServiceState::StartPending
            || service_status.current_state != ServiceState::Running
        {
            service.start(&[] as &[OsString]).context(ServiceSnafu)?;
            emit!(WindowsServiceStart {
                name: &*service_def.name.to_string_lossy(),
                already_started: false,
            });
        } else {
            emit!(WindowsServiceStart {
                name: &*service_def.name.to_string_lossy(),
                already_started: true,
            });
        }

        Ok(())
    }

    fn stop_service(service_def: &ServiceDefinition) -> crate::Result<()> {
        let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP;
        let service = open_service(service_def, service_access)?;
        let service_status = service.query_status().context(ServiceSnafu)?;

        if service_status.current_state != ServiceState::StopPending
            || service_status.current_state != ServiceState::Stopped
        {
            service.stop().context(ServiceSnafu)?;
            emit!(WindowsServiceStop {
                name: &*service_def.name.to_string_lossy(),
                already_stopped: false,
            });
        } else {
            emit!(WindowsServiceStop {
                name: &*service_def.name.to_string_lossy(),
                already_stopped: true,
            });
        }

        Ok(())
    }

    fn restart_service(
        service_def: &ServiceDefinition,
        stop_timeout: Duration,
    ) -> crate::Result<()> {
        let service_access =
            ServiceAccess::QUERY_STATUS | ServiceAccess::START | ServiceAccess::STOP;
        let service = open_service(service_def, service_access)?;
        let service_status = service.query_status().context(ServiceSnafu)?;

        if service_status.current_state == ServiceState::StartPending
            || service_status.current_state == ServiceState::Running
        {
            service.stop()?;
        }

        let service_status = ensure_state(
            &service,
            ServiceState::Stopped,
            stop_timeout,
            Duration::from_secs(1),
        )?;
        handle_service_exit_code(service_status.exit_code);

        service.start(&[] as &[OsString]).context(ServiceSnafu)?;
        emit!(WindowsServiceRestart {
            name: &*service_def.name.to_string_lossy()
        });
        Ok(())
    }

    fn install_service(service_def: &ServiceDefinition) -> crate::Result<()> {
        let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
        let service_manager =
            ServiceManager::local_computer(None::<&str>, manager_access).context(ServiceSnafu)?;

        let service_info = ServiceInfo {
            name: service_def.name.clone(),
            display_name: service_def.display_name.clone(),
            service_type: SERVICE_TYPE,
            start_type: ServiceStartType::OnDemand,
            error_control: ServiceErrorControl::Normal,
            executable_path: service_def.executable_path.clone(),
            launch_arguments: service_def.launch_arguments.clone(),
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };

        service_manager
            .create_service(&service_info, ServiceAccess::empty())
            .context(ServiceSnafu)?;

        emit!(WindowsServiceInstall {
            name: &*service_def.name.to_string_lossy(),
        });

        // TODO: It is currently not possible to change the description of the service.
        // Waiting for the following PR to get merged in
        // https://github.com/mullvad/windows-service-rs/pull/32
        //
        // service.set_description(&self.description);
        Ok(())
    }

    fn uninstall_service(service_def: &ServiceDefinition) -> crate::Result<()> {
        let service_access =
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
        let service = open_service(service_def, service_access)?;

        let service_status = service.query_status().context(ServiceSnafu)?;
        if service_status.current_state != ServiceState::Stopped {
            service.stop().context(ServiceSnafu)?;
            emit!(WindowsServiceStop {
                name: &*service_def.name.to_string_lossy(),
                already_stopped: false,
            });
        }

        let service_status = ensure_state(
            &service,
            ServiceState::Stopped,
            Duration::from_secs(10),
            Duration::from_secs(1),
        )?;
        handle_service_exit_code(service_status.exit_code);

        service.delete().context(ServiceSnafu)?;

        emit!(WindowsServiceUninstall {
            name: &*service_def.name.to_string_lossy(),
        });
        Ok(())
    }

    pub(super) fn open_service(
        service_def: &ServiceDefinition,
        access: windows_service::service::ServiceAccess,
    ) -> crate::Result<windows_service::service::Service> {
        let manager_access = ServiceManagerAccess::CONNECT;
        let service_manager =
            ServiceManager::local_computer(None::<&str>, manager_access).context(ServiceSnafu)?;

        let service = service_manager
            .open_service(&service_def.name, access)
            .map_err(|e| {
                emit!(WindowsServiceDoesNotExistError {
                    name: &*service_def.name.to_string_lossy(),
                });
                e
            })
            .context(ServiceSnafu)?;
        Ok(service)
    }

    fn handle_service_exit_code(exit_code: windows_service::service::ServiceExitCode) {
        debug!(message="Service stopped.", exit_code = ?exit_code);

        match exit_code {
            ServiceExitCode::Win32(ec) if ec != NO_ERROR => {
                warn!(message = "Service stopped with error.", exit_code = ec);
            }
            ServiceExitCode::ServiceSpecific(ec) => {
                warn!(message = "Service stopped with error.", exit_code = ec);
            }
            _ => {}
        };
    }

    fn poll_state(
        service: &windows_service::service::Service,
        state: ServiceState,
        timeout: Duration,
        wait_hint: Duration,
    ) -> Result<PollStatus> {
        let mut wait_index = 1;
        let mut wait_time = Duration::default();

        let poll_status = loop {
            let service_status = service.query_status()?;
            if service_status.current_state == state {
                break PollStatus::NoTimeout(service_status);
            }
            debug!(
                message = "Waiting for service to transition.", to = ?state, wait_index = %wait_index
            );
            wait_index += 1;

            wait_time += wait_hint;
            if wait_time >= timeout {
                break PollStatus::Timeout(service_status);
            }

            std::thread::sleep(wait_hint);
        };

        Ok(poll_status)
    }

    fn ensure_state(
        service: &windows_service::service::Service,
        state: ServiceState,
        timeout: Duration,
        wait_hint: Duration,
    ) -> crate::Result<ServiceStatus> {
        let poll_status = poll_state(service, state, timeout, wait_hint)?;

        match poll_status {
            PollStatus::Timeout(status) => Err(Error::PollTimeout {
                state: status.current_state,
                expected_state: state,
                timeout,
            }
            .into()),
            PollStatus::NoTimeout(status) => Ok(status),
        }
    }
}

define_windows_service!(ffi_service_main, win_main);

fn win_main(arguments: Vec<OsString>) {
    if let Err(_e) = run_service(arguments) {}
}

pub fn run() -> Result<i32> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main).map(|()| 0_i32)
    // Always returns 0 exit code as errors are handled by the service dispatcher.
}

fn run_service(_arguments: Vec<OsString>) -> Result<()> {
    match Application::prepare_start(Default::default()) {
        Ok((runtime, app)) => {
            let signal_tx = app.signals.handler.clone_tx();
            let event_handler = move |control_event| -> ServiceControlHandlerResult {
                match control_event {
                    // Notifies a service to report its current status information to the service
                    // control manager. Always return NoError even if not implemented.
                    ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,

                    // Handle stop
                    ServiceControl::Stop => {
                        while signal_tx.send(SignalTo::Shutdown(None)).is_err() {}
                        ServiceControlHandlerResult::NoError
                    }

                    _ => ServiceControlHandlerResult::NotImplemented,
                }
            };

            let status_handle =
                windows_service::service_control_handler::register(SERVICE_NAME, event_handler)?;

            status_handle.set_service_status(ServiceStatus {
                service_type: SERVICE_TYPE,
                current_state: ServiceState::Running,
                controls_accepted: ServiceControlAccept::STOP,
                exit_code: ServiceExitCode::Win32(NO_ERROR),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            })?;

            let program_completion_status = runtime.block_on(app.run());

            // Tell the system that service has stopped.
            status_handle.set_service_status(ServiceStatus {
                service_type: SERVICE_TYPE,
                current_state: ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: {
                    if program_completion_status.success() {
                        ServiceExitCode::Win32(NO_ERROR)
                    } else {
                        // we didn't gracefully shutdown within grace period.
                        ServiceExitCode::Win32(ERROR)
                    }
                },
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            })?;

            Ok(())
        }
        _ => Ok(()),
    }
}
