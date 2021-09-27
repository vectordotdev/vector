use super::{
    builder::ConfigBuilder, format, pipeline::Pipelines, validation, vars, Config, ConfigPath,
    Format, FormatHint,
};
use crate::signal;
use glob::glob;
use lazy_static::lazy_static;
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::Mutex,
};

lazy_static! {
    pub static ref DEFAULT_UNIX_CONFIG_PATHS: Vec<ConfigPath> = vec![ConfigPath::File(
        "/etc/vector/vector.toml".into(),
        Some(Format::Toml)
    )];
    pub static ref DEFAULT_WINDOWS_CONFIG_PATHS: Vec<ConfigPath> = {
        let program_files = std::env::var("ProgramFiles")
            .expect("%ProgramFiles% environment variable must be defined");
        let config_path = format!("{}\\Vector\\config\\vector.toml", program_files);
        vec![ConfigPath::File(
            PathBuf::from(config_path),
            Some(Format::Toml),
        )]
    };
    pub static ref CONFIG_PATHS: Mutex<Vec<ConfigPath>> = Mutex::default();
}

/// Merge the paths coming from different cli flags with different formats into
/// a unified list of paths with formats.
pub fn merge_path_lists(
    path_lists: Vec<(&[PathBuf], FormatHint)>,
) -> impl Iterator<Item = (PathBuf, FormatHint)> + '_ {
    path_lists
        .into_iter()
        .flat_map(|(paths, format)| paths.iter().cloned().map(move |path| (path, format)))
}

/// Expand a list of paths (potentially containing glob patterns) into real
/// config paths, replacing it with the default paths when empty.
pub fn process_paths(config_paths: &[ConfigPath]) -> Option<Vec<ConfigPath>> {
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

    for config_path in starting_paths {
        let config_pattern: &PathBuf = config_path.into();

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

        match config_path {
            ConfigPath::File(_, format) => {
                for path in matches {
                    paths.push(ConfigPath::File(path, *format));
                }
            }
            ConfigPath::Dir(_) => {
                for path in matches {
                    paths.push(ConfigPath::Dir(path))
                }
            }
        }
    }

    paths.sort();
    paths.dedup();
    // Ignore poison error and let the current main thread continue running to do the cleanup.
    std::mem::drop(CONFIG_PATHS.lock().map(|mut guard| *guard = paths.clone()));

    Some(paths)
}

pub fn load_from_paths(
    config_paths: &[ConfigPath],
    pipeline_paths: &[PathBuf],
) -> Result<Config, Vec<String>> {
    let (builder, load_warnings) =
        load_builder_and_pipelines_from_paths(config_paths, pipeline_paths)?;
    let (config, build_warnings) = builder.build_with_warnings()?;

    for warning in load_warnings.into_iter().chain(build_warnings) {
        warn!("{}", warning);
    }

    Ok(config)
}

/// Loads a configuration from paths. If a provider is present in the builder, the config is
/// used as bootstrapping for a remote source. Otherwise, provider instantiation is skipped.
pub async fn load_from_paths_with_provider(
    config_paths: &[ConfigPath],
    pipeline_paths: &[PathBuf],
    signal_handler: &mut signal::SignalHandler,
) -> Result<Config, Vec<String>> {
    let (mut builder, load_warnings) =
        load_builder_and_pipelines_from_paths(config_paths, pipeline_paths)?;
    validation::check_provider(&builder)?;
    signal_handler.clear();

    // If there's a provider, overwrite the existing config builder with the remote variant.
    if let Some(mut provider) = builder.provider {
        builder = provider.build(signal_handler).await?;
        debug!(message = "Provider configured.", provider = ?provider.provider_type());
    }

    let (new_config, build_warnings) = builder.build_with_warnings()?;

    for warning in load_warnings.into_iter().chain(build_warnings) {
        warn!("{}", warning);
    }

    Ok(new_config)
}

fn pipeline_paths_from_config_paths(config_paths: &[ConfigPath]) -> Vec<PathBuf> {
    config_paths
        .iter()
        .filter_map(|path| path.pipeline_dir())
        .filter(|path| path.exists())
        .collect()
}

fn load_pipelines_from_paths(pipeline_paths: &[PathBuf]) -> Result<Pipelines, Vec<String>> {
    Pipelines::load_from_paths(pipeline_paths)
}

pub fn load_builder_and_pipelines_from_paths(
    config_paths: &[ConfigPath],
    pipeline_paths: &[PathBuf],
) -> Result<(ConfigBuilder, Vec<String>), Vec<String>> {
    let pipelines = if pipeline_paths.is_empty() {
        let pipeline_paths = pipeline_paths_from_config_paths(config_paths);
        load_pipelines_from_paths(&pipeline_paths)?
    } else {
        load_pipelines_from_paths(pipeline_paths)?
    };
    let (mut builder, load_warnings) = load_builder_from_paths(config_paths)?;
    builder.set_pipelines(pipelines);

    Ok((builder, load_warnings))
}

fn load_builder_from_paths(
    config_paths: &[ConfigPath],
) -> Result<(ConfigBuilder, Vec<String>), Vec<String>> {
    let mut inputs = Vec::new();
    let mut errors = Vec::new();

    for config_path in config_paths {
        match config_path {
            ConfigPath::File(path, format) => {
                if let Some(file) = open_config(path) {
                    inputs.push((file, format.or_else(move || Format::from_path(&path).ok())));
                } else {
                    errors.push(format!("Config file not found in path: {:?}.", path));
                };
            }
            ConfigPath::Dir(path) => match path.read_dir() {
                Ok(readdir) => {
                    for res in readdir {
                        match res {
                            Ok(direntry) => {
                                // skip any unknown file formats
                                if let Ok(format) = Format::from_path(direntry.path()) {
                                    if let Some(file) = open_config(&direntry.path()) {
                                        inputs.push((file, Some(format)));
                                    }
                                }
                            }
                            Err(err) => {
                                errors.push(format!(
                                    "Could not read file in config dir: {:?}, {}.",
                                    path, err
                                ));
                            }
                        }
                    }
                }
                Err(err) => {
                    errors.push(format!("Could not read config dir: {:?}, {}.", path, err));
                }
            },
        }
    }

    if errors.is_empty() {
        load_from_inputs(inputs)
    } else {
        Err(errors)
    }
}

pub fn load_from_str(
    input: &str,
    format: FormatHint,
    pipelines: Pipelines,
) -> Result<Config, Vec<String>> {
    let (mut builder, load_warnings) =
        load_from_inputs(std::iter::once((input.as_bytes(), format)))?;
    builder.set_pipelines(pipelines);
    let (config, build_warnings) = builder.build_with_warnings()?;

    for warning in load_warnings.into_iter().chain(build_warnings) {
        warn!("{}", warning);
    }

    Ok(config)
}

fn load_from_inputs(
    inputs: impl IntoIterator<Item = (impl std::io::Read, FormatHint)>,
) -> Result<(ConfigBuilder, Vec<String>), Vec<String>> {
    let mut config = Config::builder();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for (input, format) in inputs {
        if let Err(errs) = load(input, format).and_then(|(n, mut warn)| {
            warnings.append(&mut warn);
            config.append(n)
        }) {
            // TODO: add back paths
            errors.extend(errs.iter().map(|e| e.to_string()));
        }
    }

    if errors.is_empty() {
        Ok((config, warnings))
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
                error!(message = "Error opening config file.", %error, ?path);
                None
            }
        }
    }
}

pub fn load(
    mut input: impl std::io::Read,
    format: FormatHint,
) -> Result<(ConfigBuilder, Vec<String>), Vec<String>> {
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

    format::deserialize(&with_vars, format).map(|builder| (builder, warnings))
}

#[cfg(test)]
mod tests {
    use super::load_pipelines_from_paths;
    use std::path::PathBuf;

    #[test]
    fn load_pipelines_from_tests() {
        let path = PathBuf::from("tests/pipelines/pipelines");
        let paths = vec![path];
        load_pipelines_from_paths(&paths).unwrap();
    }
}
