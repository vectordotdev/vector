use std::path::PathBuf;

use structopt::StructOpt;

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

    /// Vector config files in TOML format to be used by the service.
    #[structopt(name = "config-toml", long, use_delimiter(true))]
    config_paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format to be used by the service.
    #[structopt(name = "config-json", long, use_delimiter(true))]
    config_paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format to be used by the service.
    #[structopt(name = "config-yaml", long, use_delimiter(true))]
    config_paths_yaml: Vec<PathBuf>,

    /// The configuration files that will be used by the service.
    /// If no configuration file is specified, will target default configuration file.
    #[structopt(name = "config", short, long, use_delimiter(true))]
    config_paths: Vec<PathBuf>,

    /// Read configuration from files in one or more directories.
    /// File format is detected from the file name.
    ///
    /// Files not ending in .toml, .json, .yaml, or .yml will be ignored.
    #[structopt(
        name = "config-dir",
        short = "C",
        long,
        env = "VECTOR_CONFIG_DIR",
        use_delimiter(true)
    )]
    pub config_dirs: Vec<PathBuf>,
}

impl InstallOpts {
    fn service_info(&self) -> ServiceInfo {
        ServiceInfo {}
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
        ServiceInfo::default()
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

struct ServiceInfo {}

impl Default for ServiceInfo {
    fn default() -> Self {
        ServiceInfo {}
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
    match sub_command {
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
            error!("You must specify a sub command. Valid sub commands are [start, stop, restart, install, uninstall].");
            exitcode::USAGE
        }
    }
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
        ControlAction::Restart => vector_windows::service_control::control(
            &service_definition,
            vector_windows::service_control::ControlAction::Restart,
        ),
    };

    match res {
        Ok(()) => exitcode::OK,
        Err(error) => {
            error!(message = "Error controlling service.", %error);
            exitcode::SOFTWARE
        }
    }
}

#[cfg(unix)]
fn control_service(_service: &ServiceInfo, _action: ControlAction) -> exitcode::ExitCode {
    error!("Service commands are currently not supported on this platform.");
    exitcode::UNAVAILABLE
}
