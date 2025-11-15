mod config_builder;
mod loader;
mod secret;
mod source;

use std::{
    collections::HashMap,
    fmt::Debug,
    fs::{File, ReadDir},
    path::{Path, PathBuf},
    sync::Mutex,
};

pub use config_builder::ConfigBuilderLoader;
use glob::glob;
use loader::process::Process;
pub use loader::*;
pub use secret::*;
pub use source::*;
use vector_lib::configurable::NamedComponent;

use super::{
    Config, ConfigPath, Format, FormatHint, ProviderConfig, builder::ConfigBuilder, format,
    validation, vars,
};
use crate::signal;

pub static CONFIG_PATHS: Mutex<Vec<ConfigPath>> = Mutex::new(Vec::new());

pub(super) fn read_dir<P: AsRef<Path> + Debug>(path: P) -> Result<ReadDir, Vec<String>> {
    path.as_ref()
        .read_dir()
        .map_err(|err| vec![format!("Could not read config dir: {:?}, {}.", path, err)])
}

pub(super) fn component_name<P: AsRef<Path> + Debug>(path: P) -> Result<String, Vec<String>> {
    path.as_ref()
        .file_stem()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .ok_or_else(|| vec![format!("Couldn't get component name for file: {:?}", path)])
}

pub(super) fn open_file<P: AsRef<Path> + Debug>(path: P) -> Option<File> {
    match File::open(&path) {
        Ok(f) => Some(f),
        Err(error) => {
            if let std::io::ErrorKind::NotFound = error.kind() {
                error!(
                    message = "Config file not found in path.",
                    ?path,
                    internal_log_rate_limit = false
                );
                None
            } else {
                error!(message = "Error opening config file.", %error, ?path, internal_log_rate_limit = false);
                None
            }
        }
    }
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
    let starting_paths = if !config_paths.is_empty() {
        config_paths.to_owned()
    } else {
        default_config_paths()
    };

    let mut paths = Vec::new();

    for config_path in &starting_paths {
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
            error!(message = "Config file not found in path.", path = ?config_pattern, internal_log_rate_limit = false);
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
    drop(
        CONFIG_PATHS
            .lock()
            .map(|mut guard| guard.clone_from(&paths)),
    );

    Some(paths)
}

pub fn load_from_paths(
    config_paths: &[ConfigPath],
    interpolate_env: bool,
) -> Result<Config, Vec<String>> {
    let builder = ConfigBuilderLoader::default()
        .interpolate_env(interpolate_env)
        .load_from_paths(config_paths)?;
    let (config, build_warnings) = builder.build_with_warnings()?;

    for warning in build_warnings {
        warn!("{}", warning);
    }

    Ok(config)
}

/// Loads a configuration from paths. Handle secret replacement and if a provider is present
/// in the builder, the config is used as bootstrapping for a remote source. Otherwise,
/// provider instantiation is skipped.
pub async fn load_from_paths_with_provider_and_secrets(
    config_paths: &[ConfigPath],
    signal_handler: &mut signal::SignalHandler,
    allow_empty: bool,
    interpolate_env: bool,
) -> Result<Config, Vec<String>> {
    let secrets_backends_loader = loader_from_paths(
        SecretBackendLoader::default().interpolate_env(interpolate_env),
        config_paths,
    )?;
    let secrets = secrets_backends_loader
        .retrieve_secrets(signal_handler)
        .await
        .map_err(|e| vec![e])?;

    let mut builder = ConfigBuilderLoader::default()
        .interpolate_env(interpolate_env)
        .allow_empty(allow_empty)
        .secrets(secrets)
        .load_from_paths(config_paths)?;

    validation::check_provider(&builder)?;
    signal_handler.clear();

    // If there's a provider, overwrite the existing config builder with the remote variant.
    if let Some(mut provider) = builder.provider {
        builder = provider.build(signal_handler).await?;
        debug!(message = "Provider configured.", provider = ?provider.get_component_name());
    }

    finalize_config(builder).await
}

pub async fn load_from_str_with_secrets(
    input: &str,
    format: Format,
    signal_handler: &mut signal::SignalHandler,
    allow_empty: bool,
    interpolate_env: bool,
) -> Result<Config, Vec<String>> {
    let secrets_backends_loader = loader_from_input(
        SecretBackendLoader::default().interpolate_env(interpolate_env),
        input.as_bytes(),
        format,
    )?;
    let secrets = secrets_backends_loader
        .retrieve_secrets(signal_handler)
        .await
        .map_err(|e| vec![e])?;

    let builder = ConfigBuilderLoader::default()
        .interpolate_env(interpolate_env)
        .allow_empty(allow_empty)
        .secrets(secrets)
        .load_from_input(input.as_bytes(), format)?;
    signal_handler.clear();

    finalize_config(builder).await
}

async fn finalize_config(builder: ConfigBuilder) -> Result<Config, Vec<String>> {
    let (new_config, build_warnings) = builder.build_with_warnings()?;

    validation::check_buffer_preconditions(&new_config).await?;

    for warning in build_warnings {
        warn!("{}", warning);
    }

    Ok(new_config)
}

pub(super) fn loader_from_input<T, L, R>(
    mut loader: L,
    input: R,
    format: Format,
) -> Result<T, Vec<String>>
where
    T: serde::de::DeserializeOwned,
    L: Loader<T> + Process,
    R: std::io::Read,
{
    loader.load_from_str(input, format).map(|_| loader.take())
}

/// Iterators over `ConfigPaths`, and processes a file/dir according to a provided `Loader`.
pub(super) fn loader_from_paths<T, L>(
    mut loader: L,
    config_paths: &[ConfigPath],
) -> Result<T, Vec<String>>
where
    T: serde::de::DeserializeOwned,
    L: Loader<T> + Process,
{
    let mut errors = Vec::new();

    for config_path in config_paths {
        match config_path {
            ConfigPath::File(path, format_hint) => {
                match loader.load_from_file(
                    path,
                    format_hint
                        .or_else(move || Format::from_path(&path).ok())
                        .unwrap_or_default(),
                ) {
                    Ok(()) => {}
                    Err(errs) => errors.extend(errs),
                };
            }
            ConfigPath::Dir(path) => {
                match loader.load_from_dir(path) {
                    Ok(()) => {}
                    Err(errs) => errors.extend(errs),
                };
            }
        }
    }

    if errors.is_empty() {
        Ok(loader.take())
    } else {
        Err(errors)
    }
}

/// Uses `SourceLoader` to process `ConfigPaths`, deserializing to a toml `SourceMap`.
pub fn load_source_from_paths(
    config_paths: &[ConfigPath],
) -> Result<toml::value::Table, Vec<String>> {
    loader_from_paths(SourceLoader::new(), config_paths)
}

pub fn load_from_str(input: &str, format: Format) -> Result<Config, Vec<String>> {
    let builder = load_from_inputs(std::iter::once((input.as_bytes(), format)))?;
    let (config, build_warnings) = builder.build_with_warnings()?;

    for warning in build_warnings {
        warn!("{}", warning);
    }

    Ok(config)
}

fn load_from_inputs(
    inputs: impl IntoIterator<Item = (impl std::io::Read, Format)>,
) -> Result<ConfigBuilder, Vec<String>> {
    let mut config = Config::builder();
    let mut errors = Vec::new();

    for (input, format) in inputs {
        if let Err(errs) = load(input, format).and_then(|n| config.append(n)) {
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

pub fn prepare_input<R: std::io::Read>(
    mut input: R,
    interpolate_env: bool,
) -> Result<String, Vec<String>> {
    let mut source_string = String::new();
    input
        .read_to_string(&mut source_string)
        .map_err(|e| vec![e.to_string()])?;

    if interpolate_env {
        let mut vars: HashMap<String, String> = std::env::vars_os()
            .filter_map(|(k, v)| match (k.into_string(), v.into_string()) {
                (Ok(k), Ok(v)) => Some((k, v)),
                _ => None,
            })
            .collect();

        if !vars.contains_key("HOSTNAME")
            && let Ok(hostname) = crate::get_hostname()
        {
            vars.insert("HOSTNAME".into(), hostname);
        }
        vars::interpolate(&source_string, &vars)
    } else {
        Ok(source_string)
    }
}

pub fn load<R: std::io::Read, T>(input: R, format: Format) -> Result<T, Vec<String>>
where
    T: serde::de::DeserializeOwned,
{
    // Via configurations that load from raw string, skip interpolation of env
    let with_vars = prepare_input(input, false)?;

    format::deserialize(&with_vars, format)
}

#[cfg(not(windows))]
fn default_path() -> PathBuf {
    "/etc/vector/vector.yaml".into()
}

#[cfg(windows)]
fn default_path() -> PathBuf {
    let program_files =
        std::env::var("ProgramFiles").expect("%ProgramFiles% environment variable must be defined");
    format!("{}\\Vector\\config\\vector.yaml", program_files).into()
}

fn default_config_paths() -> Vec<ConfigPath> {
    #[cfg(not(windows))]
    let default_path = default_path();
    #[cfg(windows)]
    let default_path = default_path();

    vec![ConfigPath::File(default_path, Some(Format::Yaml))]
}
