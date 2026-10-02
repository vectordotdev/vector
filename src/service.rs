#![allow(missing_docs)]
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

use clap::Parser;

use crate::{cli::handle_config_errors, config};

const DEFAULT_SERVICE_NAME: &str = crate::built_info::PKG_NAME;

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    #[command(subcommand)]
    sub_command: Option<SubCommand>,
}

impl Opts {
    pub fn dangerously_allow_env_var_interpolation(&self) -> bool {
        matches!(
            &self.sub_command,
            Some(SubCommand::Install(opts)) if opts.dangerously_allow_env_var_interpolation
        )
    }
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

    /// Allow interpolation of environment variables in configuration files. Enabling this may
    /// expose environment secrets into your Vector configuration.
    #[arg(
        long,
        env = "VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION",
        default_value = "false"
    )]
    pub dangerously_allow_env_var_interpolation: bool,
}

impl InstallOpts {
    fn service_info(&self, data_dir: Option<&Path>) -> ServiceInfo {
        let service_name = self.name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);
        let display_name = self.display_name.as_deref().unwrap_or("Vector Service");
        let description = crate::built_info::PKG_DESCRIPTION;

        let current_exe = ::std::env::current_exe().unwrap();
        let config_paths = self.config_paths_with_formats();
        let arguments = create_service_arguments(&config_paths, data_dir).unwrap();

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
struct UninstallOpts {
    /// The name of the service.
    #[arg(long)]
    name: Option<String>,

    /// How long to wait for the service to stop before uninstalling, in seconds.
    #[arg(default_value = "10", long)]
    stop_timeout: u32,
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

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
struct StopOpts {
    /// The name of the service.
    #[arg(long)]
    name: Option<String>,

    /// How long to wait for the service to stop, in seconds.
    #[arg(default_value = "10", long)]
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

impl UninstallOpts {
    fn service_info(&self) -> ServiceInfo {
        let mut default_service = ServiceInfo::default();
        let service_name = self.name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);

        default_service.name = OsString::from(service_name);
        default_service
    }
}

impl StopOpts {
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
    Uninstall(UninstallOpts),
    /// Start the service.
    Start(StandardOpts),
    /// Stop the service.
    Stop(StopOpts),
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
    Uninstall { stop_timeout: Duration },
    Start,
    Stop { stop_timeout: Duration },
    Restart { stop_timeout: Duration },
}

pub fn cmd(opts: &Opts, data_dir: Option<&Path>) -> exitcode::ExitCode {
    let sub_command = &opts.sub_command;
    match sub_command {
        Some(s) => match s {
            SubCommand::Install(opts) => {
                control_service(&opts.service_info(data_dir), ControlAction::Install)
            }
            SubCommand::Uninstall(opts) => {
                let stop_timeout = Duration::from_secs(opts.stop_timeout as u64);
                control_service(
                    &opts.service_info(),
                    ControlAction::Uninstall { stop_timeout },
                )
            }
            SubCommand::Start(opts) => control_service(&opts.service_info(), ControlAction::Start),
            SubCommand::Stop(opts) => {
                let stop_timeout = Duration::from_secs(opts.stop_timeout as u64);
                control_service(&opts.service_info(), ControlAction::Stop { stop_timeout })
            }
            SubCommand::Restart(opts) => {
                let stop_timeout = Duration::from_secs(opts.stop_timeout as u64);
                control_service(
                    &opts.service_info(),
                    ControlAction::Restart { stop_timeout },
                )
            }
        },
        None => {
            error!(
                "You must specify a sub command. Valid sub commands are [start, stop, restart, install, uninstall]."
            );
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
        ControlAction::Uninstall { stop_timeout } => vector_windows::service_control::control(
            &service_definition,
            vector_windows::service_control::ControlAction::Uninstall { stop_timeout },
        ),
        ControlAction::Start => vector_windows::service_control::control(
            &service_definition,
            vector_windows::service_control::ControlAction::Start,
        ),
        ControlAction::Stop { stop_timeout } => vector_windows::service_control::control(
            &service_definition,
            vector_windows::service_control::ControlAction::Stop { stop_timeout },
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

fn create_service_arguments(
    config_paths: &[config::ConfigPath],
    data_dir: Option<&Path>,
) -> Option<Vec<OsString>> {
    let config_paths = config::process_paths(config_paths)?;
    match config::loading::load_from_paths_with_data_dir(&config_paths, data_dir) {
        Ok(_) => {
            let mut args: Vec<OsString> = config_paths
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
                .collect();
            if config::env_var_interpolation_enabled() {
                args.push(OsString::from("--dangerously-allow-env-var-interpolation"));
            }
            if let Some(data_dir) = data_dir {
                // Keep option and path as separate arguments. The service library owns
                // Windows command-line quoting, including spaces and trailing backslashes.
                args.push(OsString::from("--data-dir"));
                args.push(data_dir.as_os_str().to_owned());
            }
            Some(args)
        }
        Err(errs) => {
            handle_config_errors(errs);
            None
        }
    }
}

#[cfg(all(
    test,
    windows,
    feature = "sources-demo_logs",
    feature = "sinks-blackhole"
))]
mod tests {
    use super::*;

    fn write_config(path: &Path, data_dir: &Path, include_components: bool) {
        let data_dir = serde_json::to_string(data_dir).unwrap();
        let mut text = format!("data_dir: {data_dir}\n");
        if include_components {
            text.push_str("sources:\n  input:\n    type: demo_logs\n    format: shuffle\n    lines: [\"log\"]\nsinks:\n  output:\n    type: blackhole\n    inputs: [input]\n");
        }
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn service_arguments_leave_unset_data_dir_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vector config.yaml");
        write_config(&path, &directory.path().join("configured-state"), true);
        let args = create_service_arguments(&[config::ConfigPath::File(path.clone(), None)], None)
            .unwrap();
        let mut expected = vec![OsString::from("--config"), path.into_os_string()];
        if config::env_var_interpolation_enabled() {
            expected.push(OsString::from("--dangerously-allow-env-var-interpolation"));
        }
        assert_eq!(args, expected);
    }

    #[test]
    fn service_data_dir_keeps_spaces_and_backslashes_in_one_argument() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vector config.yaml");
        write_config(&path, &directory.path().join("configured-state"), true);
        let mut data_dir = directory.path().join("state with spaces");
        std::fs::create_dir(&data_dir).unwrap();
        data_dir.as_mut_os_string().push("\\");
        let args =
            create_service_arguments(&[config::ConfigPath::File(path, None)], Some(&data_dir))
                .unwrap();
        assert_eq!(args[args.len() - 2], "--data-dir");
        assert_eq!(args.last().unwrap().as_os_str(), data_dir.as_os_str());
        let parsed = crate::cli::RootOpts::try_parse_from(
            std::iter::once(OsString::from("vector")).chain(args),
        )
        .unwrap();
        assert_eq!(parsed.data_dir, Some(data_dir));
    }

    #[test]
    fn service_data_dir_override_precedes_split_config_merge() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.yaml");
        let second = directory.path().join("second.yaml");
        write_config(&first, &directory.path().join("first-state"), true);
        write_config(&second, &directory.path().join("second-state"), false);
        let paths = [
            config::ConfigPath::File(first, None),
            config::ConfigPath::File(second, None),
        ];
        assert!(create_service_arguments(&paths, None).is_none());
        let data_dir = directory.path().join("override-state");
        let args = create_service_arguments(&paths, Some(&data_dir)).unwrap();
        assert_eq!(args.last().unwrap().as_os_str(), data_dir.as_os_str());
    }
}
