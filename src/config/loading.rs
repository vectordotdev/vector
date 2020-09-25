use super::{builder::ConfigBuilder, vars, Config};
use glob::glob;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
};

lazy_static! {
    pub static ref DEFAULT_UNIX_CONFIG_PATHS: Vec<PathBuf> = vec!["/etc/vector/vector.toml".into()];
    pub static ref DEFAULT_WINDOWS_CONFIG_PATHS: Vec<PathBuf> = {
        let program_files = std::env::var("ProgramFiles")
            .expect("%ProgramFiles% environment variable must be defined");
        let config_path = format!("{}\\Vector\\config\\vector.toml", program_files);
        vec![PathBuf::from(config_path)]
    };
}

pub static CONFIG_PATHS: OnceCell<Vec<PathBuf>> = OnceCell::new();

/// Expand a list of paths (potentially containing glob patterns) into real
/// config paths, replacing it with the default paths when empty.
pub fn process_paths(config_paths: &[PathBuf]) -> Option<Vec<PathBuf>> {
    let default_paths = if cfg!(unix) {
        DEFAULT_UNIX_CONFIG_PATHS.clone()
    } else if cfg!(windows) {
        DEFAULT_WINDOWS_CONFIG_PATHS.clone()
    } else {
        DEFAULT_UNIX_CONFIG_PATHS.clone()
    };

    let starting_paths = if !config_paths.is_empty() {
        config_paths
    } else {
        &default_paths
    };

    let mut paths = Vec::new();

    for config_pattern in starting_paths {
        let matches: Vec<PathBuf> = match glob(config_pattern.to_str().expect("No ability to glob"))
        {
            Ok(glob_paths) => glob_paths.filter_map(Result::ok).collect(),
            Err(err) => {
                error!(message = "Failed to read glob pattern.", path = ?config_pattern, error = ?err);
                return None;
            }
        };

        if matches.is_empty() {
            error!(message = "Config file not found in path.", path = ?config_pattern);
            std::process::exit(exitcode::CONFIG);
        }

        for path in matches {
            paths.push(path);
        }
    }

    paths.sort();
    paths.dedup();
    CONFIG_PATHS
        .set(paths.clone())
        .expect("Cannot set global config paths");

    Some(paths)
}

pub fn load_from_paths(config_paths: &[PathBuf]) -> Result<Config, Vec<String>> {
    load_builder_from_paths(config_paths).and_then(|builder| builder.build())
}

pub(super) fn load_builder_from_paths(
    config_paths: &[PathBuf],
) -> Result<ConfigBuilder, Vec<String>> {
    let mut inputs = Vec::new();
    let mut errors = Vec::new();

    for path in config_paths {
        if let Some(file) = open_config(&path) {
            inputs.push(file);
        } else {
            errors.push(format!("Config file not found in path: {:?}.", path));
        };
    }

    if errors.is_empty() {
        load_from_inputs(inputs)
    } else {
        Err(errors)
    }
}

pub fn load_from_str(input: &str) -> Result<Config, Vec<String>> {
    load_from_inputs(std::iter::once(input.as_bytes())).and_then(|builder| builder.build())
}

fn load_from_inputs(
    inputs: impl IntoIterator<Item = impl std::io::Read>,
) -> Result<ConfigBuilder, Vec<String>> {
    let mut config = Config::builder();
    let mut errors = Vec::new();

    for input in inputs {
        if let Err(errs) = load(input).and_then(|n| config.append(n)) {
            // TODO: add back paths
            errors.extend(errs.iter().map(|e| e.to_string()));
        }
    }

    if errors.is_empty() {
        Ok(config)
    } else {
        Err(errors)
    }
}

fn open_config(path: &Path) -> Option<File> {
    match File::open(path) {
        Ok(f) => Some(f),
        Err(error) => {
            if let std::io::ErrorKind::NotFound = error.kind() {
                error!(message = "Config file not found in path.", ?path);
                None
            } else {
                error!(message = "Error opening config file.", %error);
                None
            }
        }
    }
}

fn load(mut input: impl std::io::Read) -> Result<ConfigBuilder, Vec<String>> {
    let mut source_string = String::new();
    input
        .read_to_string(&mut source_string)
        .map_err(|e| vec![e.to_string()])?;

    let mut vars = std::env::vars().collect::<HashMap<_, _>>();
    if !vars.contains_key("HOSTNAME") {
        if let Ok(hostname) = crate::get_hostname() {
            vars.insert("HOSTNAME".into(), hostname);
        }
    }
    let with_vars = vars::interpolate(&source_string, &vars);

    toml::from_str(&with_vars).map_err(|e| vec![e.to_string()])
}
