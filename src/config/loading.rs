use super::{
    builder::ConfigBuilder, format, validation, vars, ComponentKey, Config, ConfigPath,
    EnrichmentTableOuter, Format, FormatHint, SinkOuter, SourceOuter, TestDefinition,
    TransformOuter,
};
use crate::signal;
use glob::glob;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[cfg(not(windows))]
fn default_config_paths() -> Vec<ConfigPath> {
    vec![ConfigPath::File(
        "/etc/vector/vector.toml".into(),
        Some(Format::Toml),
    )]
}

#[cfg(windows)]
fn default_config_paths() -> Vec<ConfigPath> {
    let program_files =
        std::env::var("ProgramFiles").expect("%ProgramFiles% environment variable must be defined");
    let config_path = format!("{}\\Vector\\config\\vector.toml", program_files);
    vec![ConfigPath::File(
        PathBuf::from(config_path),
        Some(Format::Toml),
    )]
}

lazy_static! {
    pub static ref CONFIG_PATHS: Mutex<Vec<ConfigPath>> = Mutex::default();
}

fn component_name(path: &Path) -> Result<String, Vec<String>> {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .ok_or_else(|| vec![format!("Couldn't get component name for file: {:?}", path)])
}

trait LoadableConfig: Sized + serde::de::DeserializeOwned {
    fn load_from_file(
        path: &Path,
    ) -> Result<Option<(ComponentKey, Self, Vec<String>)>, Vec<String>> {
        let name = component_name(path).map(ComponentKey::from)?;
        if let Some(file) = open_config(path) {
            let format = Format::from_path(path).ok();
            let (component, warnings): (Self, Vec<String>) = load(file, format)?;
            Ok(Some((name, component, warnings)))
        } else {
            Ok(None)
        }
    }

    fn load_from_dir(
        path: &Path,
    ) -> Result<(IndexMap<ComponentKey, Self>, Vec<String>), Vec<String>> {
        let mut result = IndexMap::new();
        let readdir = path
            .read_dir()
            .map_err(|err| vec![format!("Could not read config dir: {:?}, {}.", path, err)])?;
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        for res in readdir {
            match res {
                Ok(direntry) => {
                    let entry_path = direntry.path();
                    if entry_path.is_file() {
                        match Self::load_from_file(&entry_path) {
                            Ok(Some((name, component, warns))) => {
                                result.insert(name, component);
                                warnings.extend(warns);
                            }
                            Ok(None) => (),
                            Err(errs) => errors.extend(errs),
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
        if errors.is_empty() {
            Ok((result, warnings))
        } else {
            Err(errors)
        }
    }

    fn load_subfolder(&mut self, _path: &Path) -> Result<Vec<String>, Vec<String>> {
        Ok(Vec::new())
    }
}

impl LoadableConfig for EnrichmentTableOuter {}
impl LoadableConfig for TransformOuter<String> {}
impl LoadableConfig for SinkOuter<String> {}
impl LoadableConfig for SourceOuter {}
impl LoadableConfig for TestDefinition {}

impl LoadableConfig for ConfigBuilder {
    fn load_subfolder(&mut self, path: &Path) -> Result<Vec<String>, Vec<String>> {
        match path.file_name().and_then(|name| name.to_str()) {
            Some("enrichment_tables") => {
                let (tables, warnings) = EnrichmentTableOuter::load_from_dir(path)?;
                self.enrichment_tables.extend(tables);
                Ok(warnings)
            }
            Some("sinks") => {
                let (sinks, warnings) = SinkOuter::load_from_dir(path)?;
                self.sinks.extend(sinks);
                Ok(warnings)
            }
            Some("sources") => {
                let (sources, warnings) = SourceOuter::load_from_dir(path)?;
                self.sources.extend(sources);
                Ok(warnings)
            }
            Some("tests") => {
                let (tests, warnings) = TestDefinition::load_from_dir(path)?;
                self.tests.extend(tests.into_iter().map(|(_, value)| value));
                Ok(warnings)
            }
            Some("transforms") => {
                let (transforms, warnings) = TransformOuter::load_from_dir(path)?;
                self.transforms.extend(transforms);
                Ok(warnings)
            }
            Some(name) => {
                // ignore hidden folders
                if name.starts_with('.') {
                    Ok(Vec::new())
                } else {
                    Err(vec![format!(
                        "Couldn't identify component type for folder {:?}",
                        path
                    )])
                }
            }
            None => Ok(Vec::new()),
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
    let default_paths = default_config_paths();

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

pub fn load_from_paths(config_paths: &[ConfigPath]) -> Result<Config, Vec<String>> {
    let (builder, load_warnings) = load_builder_from_paths(config_paths)?;
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
    signal_handler: &mut signal::SignalHandler,
) -> Result<Config, Vec<String>> {
    let (mut builder, load_warnings) = load_builder_from_paths(config_paths)?;
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

fn load_builder_from_file(
    path: &Path,
    builder: &mut ConfigBuilder,
) -> Result<Vec<String>, Vec<String>> {
    match ConfigBuilder::load_from_file(path)? {
        Some((_, loaded, warnings)) => {
            builder.append(loaded)?;
            Ok(warnings)
        }
        None => Ok(Vec::new()),
    }
}

fn load_builder_from_dir(
    path: &Path,
    builder: &mut ConfigBuilder,
) -> Result<Vec<String>, Vec<String>> {
    let readdir = path
        .read_dir()
        .map_err(|err| vec![format!("Could not read config dir: {:?}, {}.", path, err)])?;
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    for res in readdir {
        match res {
            Ok(direntry) => {
                let entry_path = direntry.path();
                if entry_path.is_file() {
                    match load_builder_from_file(&direntry.path(), builder) {
                        Ok(warns) => warnings.extend(warns),
                        Err(errs) => errors.extend(errs),
                    }
                } else if entry_path.is_dir() {
                    match builder.load_subfolder(&entry_path) {
                        Ok(warns) => warnings.extend(warns),
                        Err(errs) => errors.extend(errs),
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
    if errors.is_empty() {
        Ok(warnings)
    } else {
        Err(errors)
    }
}

pub fn load_builder_from_paths(
    config_paths: &[ConfigPath],
) -> Result<(ConfigBuilder, Vec<String>), Vec<String>> {
    let mut result = ConfigBuilder::default();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    for config_path in config_paths {
        match config_path {
            ConfigPath::File(path, _) => {
                match load_builder_from_file(path, &mut result) {
                    Ok(warns) => warnings.extend(warns),
                    Err(errs) => errors.extend(errs),
                };
            }
            ConfigPath::Dir(path) => {
                match load_builder_from_dir(path, &mut result) {
                    Ok(warns) => warnings.extend(warns),
                    Err(errs) => errors.extend(errs),
                };
            }
        }
    }

    if errors.is_empty() {
        Ok((result, warnings))
    } else {
        Err(errors)
    }
}

pub fn load_from_str(input: &str, format: FormatHint) -> Result<Config, Vec<String>> {
    let (builder, load_warnings) = load_from_inputs(std::iter::once((input.as_bytes(), format)))?;
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
        if let Err(errs) = load(input, format).and_then(|(n, warn)| {
            warnings.extend(warn);
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

pub fn load<T>(
    mut input: impl std::io::Read,
    format: FormatHint,
) -> Result<(T, Vec<String>), Vec<String>>
where
    T: serde::de::DeserializeOwned,
{
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
    use super::load_builder_from_paths;
    use crate::config::{ComponentKey, ConfigPath};
    use std::path::PathBuf;

    #[test]
    fn load_namespacing_folder() {
        let path = PathBuf::from("./tests/namespacing");
        let configs = vec![ConfigPath::Dir(path)];
        let (builder, warnings) = load_builder_from_paths(&configs).unwrap();
        assert!(warnings.is_empty());
        assert!(builder
            .transforms
            .contains_key(&ComponentKey::from("apache_parser")));
        assert!(builder
            .sources
            .contains_key(&ComponentKey::from("apache_logs")));
        assert!(builder
            .sinks
            .contains_key(&ComponentKey::from("es_cluster")));
        assert_eq!(builder.tests.len(), 2);
    }

    #[test]
    fn load_namespacing_failing() {
        let path = PathBuf::from(".").join("tests").join("namespacing-fail");
        let configs = vec![ConfigPath::Dir(path.clone())];
        let errors = load_builder_from_paths(&configs).unwrap_err();
        assert_eq!(errors.len(), 1);
        let msg = format!(
            "Couldn't identify component type for folder {:?}",
            path.join("foo")
        );
        assert_eq!(errors[0], msg);
    }
}
