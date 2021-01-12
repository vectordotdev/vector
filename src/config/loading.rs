use super::{builder::ConfigBuilder, format, handle_warnings, vars, Config, Format, FormatHint};
use glob::glob;
use lazy_static::lazy_static;
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::Mutex,
};

lazy_static! {
    pub static ref DEFAULT_UNIX_CONFIG_PATHS: Vec<(PathBuf, FormatHint)> =
        vec![("/etc/vector/vector.toml".into(), Some(Format::TOML))];
    pub static ref DEFAULT_WINDOWS_CONFIG_PATHS: Vec<(PathBuf, FormatHint)> = {
        let program_files = std::env::var("ProgramFiles")
            .expect("%ProgramFiles% environment variable must be defined");
        let config_path = format!("{}\\Vector\\config\\vector.toml", program_files);
        vec![(PathBuf::from(config_path), Some(Format::TOML))]
    };
    pub static ref CONFIG_PATHS: Mutex<Vec<(PathBuf, FormatHint)>> = Mutex::default();
}

/// Merge the paths coming from different cli flags with different formats into
/// a unified list of paths with formats.
pub fn merge_path_lists(path_lists: Vec<(&[PathBuf], FormatHint)>) -> Vec<(PathBuf, FormatHint)> {
    path_lists
        .into_iter()
        .flat_map(|(paths, format)| paths.iter().cloned().map(move |path| (path, format)))
        .collect()
}

/// Expand a list of paths (potentially containing glob patterns) into real
/// config paths, replacing it with the default paths when empty.
pub fn process_paths(config_paths: &[(PathBuf, FormatHint)]) -> Option<Vec<(PathBuf, FormatHint)>> {
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

    for (config_pattern, format) in starting_paths {
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
            paths.push((path, *format));
        }
    }

    paths.sort_by(|(a, _), (b, _)| a.cmp(b));
    paths.dedup();
    // Ignore poison error and let the current main thread continue running to do the cleanup.
    std::mem::drop(CONFIG_PATHS.lock().map(|mut guard| *guard = paths.clone()));

    Some(paths)
}

pub fn load_from_paths(
    config_paths: &[(PathBuf, FormatHint)],
    deny_warnings: bool,
) -> Result<Config, Vec<String>> {
    load_builder_from_paths(config_paths, deny_warnings)?.build_with(deny_warnings)
}

pub fn load_builder_from_paths(
    config_paths: &[(PathBuf, FormatHint)],
    deny_warnings: bool,
) -> Result<ConfigBuilder, Vec<String>> {
    let mut inputs = Vec::new();
    let mut errors = Vec::new();

    for (path, format) in config_paths {
        if let Some(file) = open_config(&path) {
            inputs.push((file, format.or_else(move || Format::from_path(&path).ok())));
        } else {
            errors.push(format!("Config file not found in path: {:?}.", path));
        };
    }

    if errors.is_empty() {
        load_from_inputs(inputs, deny_warnings)
    } else {
        Err(errors)
    }
}

pub fn load_from_str(input: &str, format: FormatHint) -> Result<Config, Vec<String>> {
    load_from_inputs(std::iter::once((input.as_bytes(), format)), false)?.build()
}

fn load_from_inputs(
    inputs: impl IntoIterator<Item = (impl std::io::Read, FormatHint)>,
    deny_warnings: bool,
) -> Result<ConfigBuilder, Vec<String>> {
    let mut config = Config::builder();
    let mut errors = Vec::new();

    for (input, format) in inputs {
        if let Err(errs) = load(input, format, deny_warnings).and_then(|n| config.append(n)) {
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
                error!(message = "Config file not found in path.", path = ?path);
                None
            } else {
                error!(message = "Error opening config file.", %error);
                None
            }
        }
    }
}

fn load(
    mut input: impl std::io::Read,
    format: FormatHint,
    deny_warnings: bool,
) -> Result<ConfigBuilder, Vec<String>> {
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
    let (with_vars, warnings) = vars::interpolate(&source_string, &vars);
    handle_warnings(warnings, deny_warnings)?;

    format::deserialize(&with_vars, format)
}
