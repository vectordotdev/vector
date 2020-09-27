use crate::app::Application;
use futures::compat::Future01CompatExt;
use std::{ffi::OsString, sync::mpsc, time::Duration};
use windows_service::service::{
    ServiceControl, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::{
    define_windows_service, service::ServiceControlAccept,
    service_control_handler::ServiceControlHandlerResult, service_dispatcher, Result,
};

const SERVICE_NAME: &str = "vector";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

pub mod service_control {
    use windows_service::service::{ServiceErrorControl, ServiceInfo, ServiceStartType};
    use windows_service::{
        service::{ServiceAccess, ServiceState},
        service_manager::{ServiceManager, ServiceManagerAccess},
        Result,
    };

    use crate::internal_events::{
        WindowsServiceDoesNotExist, WindowsServiceInstall, WindowsServiceStart, WindowsServiceStop,
        WindowsServiceUninstall,
    };
    use crate::vector_windows::SERVICE_TYPE;
    use std::ffi::OsString;
    use std::time::Duration;
    use std::{fmt, thread};

    #[derive(Debug, Clone, PartialEq)]
    pub enum ControlAction {
        Install,
        Uninstall,
        Start,
        Stop,
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

    pub fn control(service_def: &ServiceDefinition, action: ControlAction) -> Result<()> {
        match action {
            ControlAction::Start => start_service(&service_def),
            ControlAction::Stop => stop_service(&service_def),
            ControlAction::Install => install_service(&service_def),
            ControlAction::Uninstall => uninstall_service(&service_def),
        }
    }

    fn start_service(service_def: &ServiceDefinition) -> Result<()> {
        let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::START;
        let service = open_service(&service_def, service_access)?;
        let service_status = service.query_status()?;

        if service_status.current_state != ServiceState::StartPending
            || service_status.current_state != ServiceState::Running
        {
            service.start(&[] as &[OsString])?;
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

    fn stop_service(service_def: &ServiceDefinition) -> Result<()> {
        let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP;
        let service = open_service(&service_def, service_access)?;
        let service_status = service.query_status()?;

        if service_status.current_state != ServiceState::StopPending
            || service_status.current_state != ServiceState::Stopped
        {
            service.stop()?;
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

    fn install_service(service_def: &ServiceDefinition) -> Result<()> {
        let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
        let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

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

        service_manager.create_service(&service_info, ServiceAccess::empty())?;

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

    fn uninstall_service(service_def: &ServiceDefinition) -> Result<()> {
        let service_access =
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
        let service = open_service(&service_def, service_access)?;

        let service_status = service.query_status()?;
        if service_status.current_state != ServiceState::Stopped {
            service.stop()?;
            emit!(WindowsServiceStop {
                name: &*service_def.name.to_string_lossy(),
                already_stopped: false,
            });
            thread::sleep(Duration::from_secs(1));
        }

        service.delete()?;

        emit!(WindowsServiceUninstall {
            name: &*service_def.name.to_string_lossy(),
        });
        Ok(())
    }

    pub fn open_service(
        service_def: &ServiceDefinition,
        access: windows_service::service::ServiceAccess,
    ) -> Result<windows_service::service::Service> {
        let manager_access = ServiceManagerAccess::CONNECT;
        let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

        let service = service_manager
            .open_service(&service_def.name, access)
            .map_err(|e| {
                emit!(WindowsServiceDoesNotExist {
                    name: &*service_def.name.to_string_lossy(),
                });
                e
            })?;
        Ok(service)
    }
}

define_windows_service!(ffi_service_main, win_main);

fn win_main(arguments: Vec<OsString>) {
    if let Err(_e) = run_service(arguments) {}
}

pub fn run() -> Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

fn run_service(_arguments: Vec<OsString>) -> Result<()> {
    const ERROR_FAIL_SHUTDOWN: u32 = 351;

    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            // Notifies a service to report its current status information to the service
            // control manager. Always return NoError even if not implemented.
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,

            // Handle stop
            ServiceControl::Stop => {
                shutdown_tx.send(()).unwrap();
                ServiceControlHandlerResult::NoError
            }

            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };
    let status_handle =
        windows_service::service_control_handler::register(SERVICE_NAME, event_handler)?;

    let application = Application::prepare();
    let code = match application {
        Ok(app) => {
            status_handle.set_service_status(ServiceStatus {
                service_type: SERVICE_TYPE,
                current_state: ServiceState::Running,
                controls_accepted: ServiceControlAccept::STOP,
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            })?;

            let mut rt = app.runtime;
            let topology = app.config.topology;

            rt.block_on(async move {
                shutdown_rx.recv().unwrap();
                match topology.stop().compat().await {
                    Ok(()) => ServiceExitCode::NO_ERROR,
                    Err(_) => ServiceExitCode::Win32(ERROR_FAIL_SHUTDOWN),
                }
            })
        }
        Err(e) => ServiceExitCode::ServiceSpecific(e as u32),
    };

    // Tell the system that service has stopped.
    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: code,
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}
