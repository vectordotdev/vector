use std::{
    collections::{HashMap, HashSet},
    io::Read,
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::{
    ConfigPath, Format, component_name,
    interpolation::{self, ENVIRONMENT_VARIABLE_INTERPOLATION_REGEX},
    merge::ConfigNode,
    open_file, read_dir,
    representation::{ConfigMap, deserialize_config_value, parse_config_value},
    schema_coercion::ValueCoercer,
    secret::{self, COLLECTOR, SECRET_KEY},
};

/// Provides a hint to the loading system of the type of components that should be found
/// when traversing an explicitly named directory.
#[derive(Debug, Copy, Clone)]
pub enum ComponentHint {
    Source,
    Transform,
    Sink,
    Test,
    EnrichmentTable,
}

impl ComponentHint {
    /// Returns the component string field that should host a component -- e.g. sources,
    /// transforms, etc.
    pub(super) const fn as_component_field(self) -> &'static str {
        match self {
            ComponentHint::Source => "sources",
            ComponentHint::Transform => "transforms",
            ComponentHint::Sink => "sinks",
            ComponentHint::Test => "tests",
            ComponentHint::EnrichmentTable => "enrichment_tables",
        }
    }

    /// Joins a component sub-folder to a provided path, for traversal. Since `Self` is a
    /// `Copy`, this is more efficient to pass by value than ref.
    #[must_use]
    pub fn join_path(self, path: &Path) -> PathBuf {
        path.join(self.as_component_field())
    }
}

/// How an assembled map contributes to the configuration.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ConfigScope {
    Root,
    Component(ComponentHint),
}

#[derive(Clone, Copy)]
pub(super) enum LoadMode {
    Raw,
    SecretBackends,
    Configuration,
}

/// Assembly has already happened. Consumers choose which values to resolve and
/// how each original file/directory boundary contributes to the final result.
pub(super) trait ConfigLoader {
    const MODE: LoadMode;

    fn visit_string(&mut self, _value: &str) {}

    fn merge(&mut self, map: ConfigMap, scope: ConfigScope) -> Result<(), Vec<String>>;
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DocumentOrigin {
    path: Option<PathBuf>,
    format: Format,
}

#[derive(Debug)]
struct ConfigBatch {
    scope: ConfigScope,
    tree: Result<ConfigNode, Vec<String>>,
}

/// A single filesystem snapshot, already assembled into merge trees. Separate
/// inputs keep their append boundaries; each scalar keeps all its contributions.
#[derive(Debug, Default)]
pub(crate) struct ParsedInputs {
    inputs: Vec<Vec<ConfigBatch>>,
    environment_interpolated: bool,
    secrets_substituted: bool,
    #[cfg(test)]
    documents: Vec<DocumentOrigin>,
}

impl ParsedInputs {
    pub(crate) fn from_paths(paths: &[ConfigPath]) -> Self {
        InputReader::default().read_paths(paths)
    }

    pub(crate) fn from_input(input: impl Read, format: Format) -> Self {
        Self {
            inputs: vec![vec![ConfigBatch {
                scope: ConfigScope::Root,
                tree: parse_document(input, format).map(ConfigNode::from_map),
            }]],
            ..Self::default()
        }
    }

    pub(crate) fn interpolate_environment(&mut self, enabled: bool) {
        if self.environment_interpolated {
            return;
        }
        self.environment_interpolated = true;
        if enabled {
            let vars = environment_variables();
            self.interpolate_strings(|value| interpolation::interpolate(value, &vars));
        }
    }

    pub(crate) fn substitute_secrets(&mut self, secrets: &HashMap<String, String>) {
        if self.secrets_substituted {
            return;
        }
        self.secrets_substituted = true;
        if !secrets.is_empty() {
            self.interpolate_strings(|value| secret::interpolate(value, secrets));
        }
    }

    fn interpolate_strings(&mut self, interpolate: impl Fn(&str) -> Result<String, Vec<String>>) {
        for batch in self.inputs.iter_mut().flatten() {
            if let Ok(tree) = &mut batch.tree
                && let Err(errors) = tree.interpolate(&interpolate)
            {
                batch.tree = Err(errors);
            }
        }
    }

    pub(super) fn load_into<L: ConfigLoader>(&self, loader: &mut L) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let mut schema = None;
        for input in &self.inputs {
            for batch in input {
                let result = batch.tree.as_ref().map_err(Clone::clone).and_then(|tree| {
                    tree.visit_strings(&mut |value| loader.visit_string(value));
                    let map = match L::MODE {
                        LoadMode::Raw => deserialize_config_value(tree.raw()?)?,
                        LoadMode::SecretBackends => {
                            // Component secrets remain unresolved during backend discovery.
                            let Some(backends) = tree.fields().get(SECRET_KEY) else {
                                return loader.merge(ConfigMap::new(), batch.scope);
                            };
                            let projection = ConfigNode::Object(
                                [(SECRET_KEY.to_owned(), backends.clone())]
                                    .into_iter()
                                    .collect(),
                            );
                            coerce_tree(&projection, &mut schema)?
                        }
                        LoadMode::Configuration => resolve_batch(tree, batch.scope, &mut schema)?,
                    };
                    loader.merge(map, batch.scope)
                });
                if let Err(failures) = result {
                    errors.extend(failures);
                    // Preserve the root-before-components boundary of each directory.
                    break;
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn resolve_batch(
    tree: &ConfigNode,
    scope: ConfigScope,
    schema: &mut Option<Result<Value, Vec<String>>>,
) -> Result<ConfigMap, Vec<String>> {
    let ConfigScope::Component(hint) = scope else {
        return coerce_tree(tree, schema);
    };
    let key = hint.as_component_field();
    let names: Vec<_> = tree.fields().keys().cloned().collect();
    let components = if matches!(hint, ComponentHint::Test) {
        ConfigNode::Array(tree.fields().values().cloned().collect())
    } else {
        tree.clone()
    };
    let root = ConfigNode::Object([(key.to_owned(), components)].into_iter().collect());
    let mut root = coerce_tree(&root, schema)?;
    let value = root.remove(key).expect("component field survives coercion");
    if matches!(hint, ComponentHint::Test) {
        let values: Vec<Value> = deserialize_config_value(value)?;
        Ok(names.into_iter().zip(values).collect())
    } else {
        deserialize_config_value(value)
    }
}

fn coerce_tree(
    tree: &ConfigNode,
    schema: &mut Option<Result<Value, Vec<String>>>,
) -> Result<ConfigMap, Vec<String>> {
    let schema = schema
        .get_or_insert_with(config_schema)
        .as_ref()
        .map_err(Clone::clone)?;
    let value = ValueCoercer::new(schema)
        .coerce_tree(tree)
        .map_err(|error| vec![error.to_string()])?;
    deserialize_config_value(value)
}

#[derive(Clone, Default)]
struct DirectoryListing {
    files: Vec<PathBuf>,
    folders: Vec<PathBuf>,
    errors: Vec<String>,
}

/// Files and listings are read once. Directory traversal builds the merge tree
/// directly, using inline keys to skip shadowed folders before opening them.
#[derive(Default)]
struct InputReader {
    documents: HashMap<DocumentOrigin, Option<Result<ConfigNode, Vec<String>>>>,
    listings: HashMap<PathBuf, Result<DirectoryListing, Vec<String>>>,
    active_directories: HashSet<(PathBuf, Vec<String>)>,
}

impl InputReader {
    fn read_paths(mut self, paths: &[ConfigPath]) -> ParsedInputs {
        let inputs = paths
            .iter()
            .map(|path| match path {
                ConfigPath::File(path, format) => {
                    let format = format
                        .or_else(|| Format::from_path(path).ok())
                        .unwrap_or_default();
                    self.file(path, format)
                        .map(|(_, tree)| ConfigBatch {
                            scope: ConfigScope::Root,
                            tree,
                        })
                        .into_iter()
                        .collect()
                }
                ConfigPath::Dir(path) => {
                    let root =
                        self.directory(path, false, ConfigNode::object())
                            .and_then(|files| {
                                let mut root = ConfigNode::object();
                                for file in files.into_fields().into_values() {
                                    root.merge(file)?;
                                }
                                Ok(root)
                            });
                    let mut batches = vec![ConfigBatch {
                        scope: ConfigScope::Root,
                        tree: root,
                    }];
                    for hint in [
                        ComponentHint::Source,
                        ComponentHint::Transform,
                        ComponentHint::Sink,
                        ComponentHint::Test,
                        ComponentHint::EnrichmentTable,
                    ] {
                        let path = hint.join_path(path);
                        if path.is_dir() {
                            batches.push(ConfigBatch {
                                scope: ConfigScope::Component(hint),
                                tree: self.directory(
                                    &path,
                                    matches!(hint, ComponentHint::Transform),
                                    ConfigNode::object(),
                                ),
                            });
                        }
                    }
                    batches
                }
            })
            .collect();
        ParsedInputs {
            inputs,
            #[cfg(test)]
            documents: self
                .documents
                .into_iter()
                .filter_map(|(origin, value)| value.map(|_| origin))
                .collect(),
            ..ParsedInputs::default()
        }
    }

    fn file(
        &mut self,
        path: &Path,
        format: Format,
    ) -> Option<(String, Result<ConfigNode, Vec<String>>)> {
        let name = component_name(path).ok()?;
        let origin = DocumentOrigin {
            path: Some(path.to_owned()),
            format,
        };
        let document = self.documents.entry(origin).or_insert_with(|| {
            open_file(path).map(|input| parse_document(input, format).map(ConfigNode::from_map))
        });
        document.clone().map(|tree| (name, tree))
    }

    fn directory(
        &mut self,
        path: &Path,
        recurse: bool,
        initial: ConfigNode,
    ) -> Result<ConfigNode, Vec<String>> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        let mut keys = initial.fields().keys().cloned().collect::<Vec<_>>();
        keys.sort_unstable();
        let context = (canonical, keys);
        if !self.active_directories.insert(context.clone()) {
            return Err(vec![format!(
                "Configuration directory contains a symbolic-link cycle: {path:?}."
            )]);
        }
        let result = self.read_directory(path, recurse, initial);
        self.active_directories.remove(&context);
        result
    }

    fn read_directory(
        &mut self,
        path: &Path,
        recurse: bool,
        mut result: ConfigNode,
    ) -> Result<ConfigNode, Vec<String>> {
        let listing = self
            .listings
            .entry(path.to_owned())
            .or_insert_with(|| Self::list_directory(path))
            .clone()?;
        let mut errors = listing.errors;
        for path in listing.files {
            let Ok(format) = Format::from_path(&path) else {
                continue;
            };
            if let Some((name, document)) = self.file(&path, format) {
                let document = document.and_then(|tree| {
                    let nested = path.parent().map(|parent| parent.join(&name));
                    if recurse && let Some(nested) = nested.filter(|path| path.is_dir()) {
                        self.directory(&nested, true, tree)
                    } else {
                        Ok(tree)
                    }
                });
                match document {
                    Ok(tree) => {
                        if let Err(failures) =
                            result.merge(ConfigNode::Object([(name, tree)].into_iter().collect()))
                        {
                            errors.extend(failures);
                        }
                    }
                    Err(failures) => errors.extend(failures),
                }
            }
        }
        if recurse {
            for path in listing.folders {
                if let Ok(name) = component_name(&path)
                    && !result.fields().contains_key(&name)
                {
                    match self.directory(&path, true, ConfigNode::object()) {
                        Ok(tree) => {
                            result
                                .merge(ConfigNode::Object([(name, tree)].into_iter().collect()))?;
                        }
                        Err(failures) => errors.extend(failures),
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(result)
        } else {
            Err(errors)
        }
    }

    #[expect(
        clippy::unnecessary_debug_formatting,
        reason = "Keep paths quoted and escaped in diagnostics"
    )]
    fn list_directory(path: &Path) -> Result<DirectoryListing, Vec<String>> {
        let mut listing = DirectoryListing::default();
        for entry in read_dir(path)? {
            match entry {
                Ok(entry) => {
                    let entry = entry.path();
                    if entry.is_file() {
                        listing.files.push(entry);
                    } else if entry.is_dir()
                        && !entry
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with('.'))
                    {
                        listing.folders.push(entry);
                    }
                }
                Err(error) => listing.errors.push(format!(
                    "Could not read entry in config dir: {path:?}, {error}."
                )),
            }
        }
        Ok(listing)
    }
}

fn parse_document(input: impl Read, format: Format) -> Result<ConfigMap, Vec<String>> {
    let source = string_from_input(input)?;
    let value = parse_config_value(&source, format).map_err(|mut errors| {
        if ENVIRONMENT_VARIABLE_INTERPOLATION_REGEX.is_match(&source) || COLLECTOR.is_match(&source)
        {
            errors.push(
                "Configuration is parsed before interpolation. Quote placeholders in \
                 TOML and JSON values and inside YAML flow collections ([...] and {...}); \
                 they will be coerced to the field's declared type after substitution."
                    .to_owned(),
            );
        }
        errors
    })?;
    deserialize_config_value(value)
}

/// Deserialization follows resolution; Serde owns component validation and defaults.
pub(super) fn deserialize_config_map<T: serde::de::DeserializeOwned>(
    map: ConfigMap,
) -> Result<T, Vec<String>> {
    deserialize_config_value(Value::Object(map))
}

pub(super) fn deserialize_component_map<T: serde::de::DeserializeOwned>(
    map: ConfigMap,
) -> Result<indexmap::IndexMap<crate::config::ComponentKey, T>, Vec<String>> {
    deserialize_config_map(map)
}

fn config_schema() -> Result<Value, Vec<String>> {
    let schema = vector_config::schema::generate_root_schema::<crate::config::ConfigBuilder>()
        .map_err(|error| vec![format!("{error:?}")])?;
    serde_json::to_value(schema).map_err(|error| vec![error.to_string()])
}

pub(super) fn string_from_input<R: Read>(mut input: R) -> Result<String, Vec<String>> {
    let mut source = String::new();
    input
        .read_to_string(&mut source)
        .map_err(|error| vec![error.to_string()])?;
    Ok(source)
}

fn environment_variables() -> HashMap<String, String> {
    let mut vars = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect::<HashMap<_, _>>();
    if !vars.contains_key("HOSTNAME")
        && let Ok(hostname) = crate::get_hostname()
    {
        vars.insert("HOSTNAME".into(), hostname);
    }
    vars
}

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "sources-demo_logs"))]
mod merge_tests;
