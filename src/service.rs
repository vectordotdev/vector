#![allow(missing_docs)]
use std::{ffi::OsString, path::PathBuf, time::Duration};

use clap::Parser;

use crate::{cli::handle_config_errors, config};

const DEFAULT_SERVICE_NAME: &str = crate::built_info::PKG_NAME;

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    #[command(subcommand)]
    sub_command: Option<SubCommand>,
}

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
struct InstallOpts {
    /// The name of the service to install.
    #[arg(long)]
    name: Option<String>,

    /// The display name to be used by interface programs to identify the service like Windows Services App
    #[arg(long)]
    display_name: Option<String>,

    /// Vector config files in TOML format to be used by the service.
    #[arg(name = "config-toml", long, value_delimiter(','))]
    config_paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format to be used by the service.
    #[arg(name = "config-json", long, value_delimiter(','))]
    config_paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format to be used by the service.
    #[arg(name = "config-yaml", long, value_delimiter(','))]
    config_paths_yaml: Vec<PathBuf>,

    /// The configuration files that will be used by the service.
    /// If no configuration file is specified, will target default configuration file.
    #[arg(name = "config", short, long, value_delimiter(','))]
    config_paths: Vec<PathBuf>,

    /// Read configuration from files in one or more directories.
    /// File format is detected from the file name.
    ///
    /// Files not ending in .toml, .json, .yaml, or .yml will be ignored.
    #[arg(
        id = "config-dir",
        short = 'C',
        long,
        env = "VECTOR_CONFIG_DIR",
        value_delimiter(',')
    )]
    config_dirs: Vec<PathBuf>,
}

impl InstallOpts {
    fn service_info(&self) -> ServiceInfo {
        let service_name = self.name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);
        let display_name = self.display_name.as_deref().unwrap_or("Vector Service");
        let description = crate::built_info::PKG_DESCRIPTION;

        let current_exe = ::std::env::current_exe().unwrap();
        let config_paths = self.config_paths_with_formats();
        let arguments = create_service_arguments(&config_paths).unwrap();

        ServiceInfo {
            name: OsString::from(service_name),
            display_name: OsString::from(display_name),
            description: OsString::from(description),
            executable_path: current_exe,
            launch_arguments: arguments,
        }
    }

    fn config_paths_with_formats(&self) -> Vec<config::ConfigPath> {
        config::merge_path_lists(vec![
            (&self.config_paths, None),
            (&self.config_paths_toml, Some(config::Format::Toml)),
            (&self.config_paths_json, Some(config::Format::Json)),
            (&self.config_paths_yaml, Some(config::Format::Yaml)),
        ])
        .map(|(path, hint)| config::ConfigPath::File(path, hint))
        .chain(
            self.config_dirs
                .iter()
                .map(|dir| config::ConfigPath::Dir(dir.to_path_buf())),
        )
        .collect()
    }
}

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
struct RestartOpts {
    /// The name of the service.
    #[arg(long)]
    name: Option<String>,

    /// How long to wait for the service to stop before starting it back, in seconds.
    #[arg(default_value = "60", long)]
    stop_timeout: u32,
}

impl RestartOpts {
    fn service_info(&self) -> ServiceInfo {
        let mut default_service = ServiceInfo::default();
        let service_name = self.name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);

        default_service.name = OsString::from(service_name);
        default_service
    }
}

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
struct StandardOpts {
    /// The name of the service.
    #[arg(long)]
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

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
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
    Restart(RestartOpts),
}

struct ServiceInfo {
    name: OsString,
    display_name: OsString,
    description: OsString,

    executable_path: std::path::PathBuf,
    launch_arguments: Vec<OsString>,
}

impl Default for ServiceInfo {
    fn default() -> Self {
        let current_exe = ::std::env::current_exe().unwrap();

        ServiceInfo {
            name: OsString::from(DEFAULT_SERVICE_NAME),
            display_name: OsString::from("Vector Service"),
            description: OsString::from(crate::built_info::PKG_DESCRIPTION),
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
    Restart { stop_timeout: Duration },
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
                let stop_timeout = Duration::from_secs(opts.stop_timeout as u64);
                control_service(
                    &opts.service_info(),
                    ControlAction::Restart { stop_timeout },
                )
            }
        },
        None => {
            error!("You must specify a sub command. Valid sub commands are [start, stop, restart, install, uninstall].");
            exitcode::USAGE
        }
    }
}

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
        ControlAction::Restart { stop_timeout } => vector_windows::service_control::control(
            &service_definition,
            vector_windows::service_control::ControlAction::Restart { stop_timeout },
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

fn create_service_arguments(config_paths: &[config::ConfigPath]) -> Option<Vec<OsString>> {
    let config_paths = config::process_paths(config_paths)?;
    match config::load_from_paths(&config_paths) {
        Ok(_) => Some(
            config_paths
                .iter()
                .flat_map(|config_path| match config_path {
                    config::ConfigPath::File(path, format) => {
                        let key = match format {
                            None => "--config",
                            Some(config::Format::Toml) => "--config-toml",
                            Some(config::Format::Json) => "--config-json",
                            Some(config::Format::Yaml) => "--config-yaml",
                        };
                        vec![OsString::from(key), path.as_os_str().into()]
                    }
                    config::ConfigPath::Dir(path) => {
                        vec![OsString::from("--config-dir"), path.as_os_str().into()]
                    }
                })
                .collect::<Vec<OsString>>(),
        ),
        Err(errs) => {
            handle_config_errors(errs);
            None
        }
    }
}
