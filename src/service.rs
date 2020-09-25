use std::ffi::OsString;
use std::path::PathBuf;

use structopt::StructOpt;

use crate::cli::handle_config_errors;
use crate::config;

const DEFAULT_SERVICE_NAME: &'static str = built_info::PKG_NAME;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    #[structopt(subcommand)]
    sub_command: Option<SubCommand>,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct InstallOpts {
    /// The name of the service to install.
    #[structopt(long)]
    name: Option<String>,

    /// The display name to be used by interface programs to identify the service like Windows Services App
    #[structopt(long)]
    display_name: Option<String>,

    /// The configuration files that will be used by the service.
    /// If no configuration file is specified, will target default configuration file.
    #[structopt(name = "config", short, long)]
    config_paths: Vec<PathBuf>,
}

impl InstallOpts {
    fn service_info(&self) -> ServiceInfo {
        let service_name = self.name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);
        let display_name = self.display_name.as_deref().unwrap_or("Vector Service");
        let description = built_info::PKG_DESCRIPTION;

        let current_exe = ::std::env::current_exe().unwrap();
        let arguments = create_service_arguments(&self.config_paths).unwrap();

        ServiceInfo {
            name: OsString::from(service_name),
            display_name: OsString::from(display_name),
            description: OsString::from(description),
            executable_path: current_exe,
            launch_arguments: arguments,
        }
    }
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct StandardOpts {
    /// The name of the service.
    #[structopt(long)]
    name: Option<String>,
}

impl StandardOpts {
    fn service_info(&self) -> ServiceInfo {
        let mut default_service = ServiceInfo::default();
        let service_name = self.name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);

        default_service.name = OsString::from(service_name);
        default_service
    }
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
enum SubCommand {
    /// Install the service.
    Install(InstallOpts),
    /// Uninstall the service.
    Uninstall(StandardOpts),
    /// Start the service.
    Start(StandardOpts),
    /// Stop the service.
    Stop(StandardOpts),
    /// Restart the service.
    Restart(StandardOpts),
}

struct ServiceInfo {
    pub name: OsString,
    pub display_name: OsString,
    pub description: OsString,

    pub executable_path: std::path::PathBuf,
    pub launch_arguments: Vec<OsString>,
}

impl Default for ServiceInfo {
    fn default() -> Self {
        let current_exe = ::std::env::current_exe().unwrap();

        ServiceInfo {
            name: OsString::from(DEFAULT_SERVICE_NAME),
            display_name: OsString::from("Vector Service"),
            description: OsString::from(built_info::PKG_DESCRIPTION),
            executable_path: current_exe,
            launch_arguments: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ControlAction {
    Install,
    Uninstall,
    Start,
    Stop,
    Restart,
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let sub_command = &opts.sub_command;
    let code = match sub_command {
        Some(s) => match s {
            SubCommand::Install(opts) => {
                control_service(&opts.service_info(), ControlAction::Install)
            }
            SubCommand::Uninstall(opts) => {
                control_service(&opts.service_info(), ControlAction::Uninstall)
            }
            SubCommand::Start(opts) => control_service(&opts.service_info(), ControlAction::Start),
            SubCommand::Stop(opts) => control_service(&opts.service_info(), ControlAction::Stop),
            SubCommand::Restart(opts) => {
                control_service(&opts.service_info(), ControlAction::Restart)
            }
        },
        None => {
            error!("You must specify a sub command. Valid sub commands are [start, stop, restart, install, uninstall]");
            exitcode::USAGE
        }
    };

    code
}

#[cfg(windows)]
fn control_service(service: &ServiceInfo, action: ControlAction) -> exitcode::ExitCode {
    use crate::vector_windows;

    let service_definition = vector_windows::service_control::ServiceDefinition {
        name: service.name.clone(),
        display_name: service.display_name.clone(),
        description: service.description.clone(),
        executable_path: service.executable_path.clone(),
        launch_arguments: service.launch_arguments.clone(),
    };

    let res = match action {
        ControlAction::Install => vector_windows::service_control::control(
            &service_definition,
            vector_windows::service_control::ControlAction::Install,
        ),
        ControlAction::Uninstall => vector_windows::service_control::control(
            &service_definition,
            vector_windows::service_control::ControlAction::Uninstall,
        ),
        ControlAction::Start => vector_windows::service_control::control(
            &service_definition,
            vector_windows::service_control::ControlAction::Start,
        ),
        ControlAction::Stop => vector_windows::service_control::control(
            &service_definition,
            vector_windows::service_control::ControlAction::Stop,
        ),
        _ => unreachable!(),
    };

    let exit_code = match res {
        Ok(()) => exitcode::OK,
        Err(err) => {
            if let windows_service::Error::Winapi(win_err) = err {
                error!(message="Error controlling service.", %win_err)
            } else {
                error!(message="Error controlling service.", %err)
            }
            exitcode::SOFTWARE
        }
    };

    exit_code
}

#[cfg(unix)]
fn control_service(_service: &ServiceInfo, _action: ControlAction) -> exitcode::ExitCode {
    error!("Service commands are currently not supported on this platform.");
    exitcode::UNAVAILABLE
}

fn create_service_arguments(config_paths: &Vec<PathBuf>) -> Option<Vec<OsString>> {
    let config_paths = config::process_paths(&config_paths)?;
    match config::load_from_paths(&config_paths) {
        Ok(_) => Some(
            config_paths
                .iter()
                .flat_map(|p| vec![OsString::from("--config"), p.as_os_str().into()])
                .collect::<Vec<OsString>>(),
        ),
        Err(errs) => {
            handle_config_errors(errs);
            None
        }
    }
}

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
