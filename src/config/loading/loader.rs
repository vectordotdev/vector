use std::{
    collections::{HashMap, HashSet},
    io::Read,
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::{
    ConfigPath, Format, component_name, interpolate_config_map_with_env_vars,
    interpolation::ENVIRONMENT_VARIABLE_INTERPOLATION_REGEX,
    open_file, read_dir,
    representation::{
        ConfigMap, DeferredConflict, deserialize_config_value, merge_maps_deferred, merge_values,
        merge_values_deferred, parse_config_value, resolve_merge_conflicts,
    },
    schema_coercion::ValueCoercer,
    secret::{COLLECTOR, SECRET_KEY, interpolate_config_map_with_secrets},
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
    DirectoryRoot,
    Component(ComponentHint),
}

/// Secret discovery cannot validate component values before their secrets are resolved.
#[derive(Clone, Copy)]
pub(super) enum CoercionScope {
    Configuration,
    SecretBackends,
}

type DocumentId = usize;
type DirectoryId = usize;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DocumentOrigin {
    path: Option<PathBuf>,
    format: Format,
}

#[derive(Debug)]
struct ParsedDocument {
    origin: DocumentOrigin,
    value: Result<ConfigMap, Vec<String>>,
}

#[derive(Debug)]
enum ParsedInput {
    File(DocumentId),
    Directory {
        root: DirectoryId,
        components: Vec<(ComponentHint, DirectoryId)>,
    },
}

#[derive(Debug)]
struct NamedFile {
    name: String,
    document: DocumentId,
    nested: Option<DirectoryId>,
}

#[derive(Debug, Default)]
struct ParsedDirectory {
    files: Vec<NamedFile>,
    folders: Vec<(String, DirectoryId)>,
    errors: Vec<String>,
}

/// Parsed documents and their original assembly boundaries, retained across secret resolution.
///
/// Documents are not merged here: substitutions must precede same-name file merges, and
/// secret discovery must still see references in values that a later file overwrites.
#[derive(Debug, Default)]
pub(crate) struct ParsedInputs {
    documents: Vec<ParsedDocument>,
    directories: Vec<Result<ParsedDirectory, Vec<String>>>,
    inputs: Vec<ParsedInput>,
    environment_interpolated: bool,
    secrets_substituted: bool,
}

impl ParsedInputs {
    pub(crate) fn from_paths(paths: &[ConfigPath]) -> Self {
        let mut reader = InputReader::default();
        for path in paths {
            match path {
                ConfigPath::File(path, format) => {
                    let format = format
                        .or_else(|| Format::from_path(path).ok())
                        .unwrap_or_default();
                    if let Some((_, document)) = reader.file(path, format) {
                        reader.parsed.inputs.push(ParsedInput::File(document));
                    }
                }
                ConfigPath::Dir(path) => {
                    let root = reader.directory(path, false, &ConfigMap::new());
                    let mut components = Vec::new();
                    for hint in [
                        ComponentHint::Source,
                        ComponentHint::Transform,
                        ComponentHint::Sink,
                        ComponentHint::Test,
                        ComponentHint::EnrichmentTable,
                    ] {
                        let path = hint.join_path(path);
                        if path.exists() && path.is_dir() {
                            let directory = reader.directory(
                                &path,
                                matches!(hint, ComponentHint::Transform),
                                &ConfigMap::new(),
                            );
                            components.push((hint, directory));
                        }
                    }
                    reader
                        .parsed
                        .inputs
                        .push(ParsedInput::Directory { root, components });
                }
            }
        }
        reader.parsed
    }

    pub(crate) fn from_input(input: impl Read, format: Format) -> Self {
        Self {
            documents: vec![ParsedDocument {
                origin: DocumentOrigin { path: None, format },
                value: parse_document(input, format),
            }],
            inputs: vec![ParsedInput::File(0)],
            ..Self::default()
        }
    }

    /// Applies one environment snapshot to every retained document. Errors remain attached
    /// to documents, so an ignored recursive folder cannot make its parent configuration fail.
    pub(crate) fn interpolate_environment(&mut self, enabled: bool) {
        if self.environment_interpolated {
            return;
        }
        self.environment_interpolated = true;
        if enabled {
            let vars = environment_variables();
            self.update_documents(|map| interpolate_config_map_with_env_vars(map, &vars));
        }
    }

    pub(crate) fn substitute_secrets(&mut self, secrets: &HashMap<String, String>) {
        if self.secrets_substituted {
            return;
        }
        self.secrets_substituted = true;
        if !secrets.is_empty() {
            self.update_documents(|map| interpolate_config_map_with_secrets(map, secrets));
        }
    }

    fn update_documents(
        &mut self,
        mut update: impl FnMut(&ConfigMap) -> Result<ConfigMap, Vec<String>>,
    ) {
        for document in &mut self.documents {
            if let Ok(map) = &document.value {
                document.value = update(map);
            }
        }
    }

    /// Replays the original assembly policy without filesystem access. The observer sees
    /// each visited document before merging can discard any of its values.
    pub(crate) fn assemble(
        &self,
        visit_document: impl FnMut(&ConfigMap),
        merge: impl FnMut(ConfigMap, ConfigScope) -> Result<(), Vec<String>>,
    ) -> Result<(), Vec<String>> {
        self.assemble_with(None, visit_document, merge)
    }

    pub(super) fn assemble_coerced(
        &self,
        scope: CoercionScope,
        visit_document: impl FnMut(&ConfigMap),
        merge: impl FnMut(ConfigMap, ConfigScope) -> Result<(), Vec<String>>,
    ) -> Result<(), Vec<String>> {
        self.assemble_with(Some(scope), visit_document, merge)
    }

    fn assemble_with(
        &self,
        coercion: Option<CoercionScope>,
        mut visit_document: impl FnMut(&ConfigMap),
        mut merge: impl FnMut(ConfigMap, ConfigScope) -> Result<(), Vec<String>>,
    ) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        for input in &self.inputs {
            if let Err(failures) =
                self.assemble_input(input, coercion, &mut visit_document, &mut merge)
            {
                errors.extend(failures);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn assemble_input(
        &self,
        input: &ParsedInput,
        coercion: Option<CoercionScope>,
        visit_document: &mut impl FnMut(&ConfigMap),
        merge: &mut impl FnMut(ConfigMap, ConfigScope) -> Result<(), Vec<String>>,
    ) -> Result<(), Vec<String>> {
        match input {
            ParsedInput::File(document) => {
                merge(self.document(*document, visit_document)?, ConfigScope::Root)
            }
            ParsedInput::Directory { root, components } => {
                let mut root_map = ConfigMap::new();
                let mut conflicts = Vec::new();
                self.assemble_directory(
                    *root,
                    &mut root_map,
                    visit_document,
                    coercion.map(|_| &mut conflicts),
                )?;
                if let Some(coercion) = coercion {
                    merge(
                        resolve_directory_root(root_map, conflicts, coercion)?,
                        ConfigScope::Root,
                    )?;
                } else {
                    merge(root_map, ConfigScope::DirectoryRoot)?;
                }
                for (hint, directory) in components {
                    let mut map = ConfigMap::new();
                    let mut conflicts = Vec::new();
                    self.assemble_directory(
                        *directory,
                        &mut map,
                        visit_document,
                        coercion.map(|_| &mut conflicts),
                    )?;
                    if let Some(coercion) = coercion {
                        map = resolve_component_conflicts(map, conflicts, *hint, coercion)?;
                    }
                    merge(map, ConfigScope::Component(*hint))?;
                }
                Ok(())
            }
        }
    }

    fn document(
        &self,
        document: DocumentId,
        visit_document: &mut impl FnMut(&ConfigMap),
    ) -> Result<ConfigMap, Vec<String>> {
        let map = self.documents[document]
            .value
            .as_ref()
            .map_err(Clone::clone)?;
        visit_document(map);
        Ok(map.clone())
    }

    fn assemble_directory(
        &self,
        directory: DirectoryId,
        result: &mut ConfigMap,
        visit_document: &mut impl FnMut(&ConfigMap),
        mut conflicts: Option<&mut Vec<DeferredConflict>>,
    ) -> Result<(), Vec<String>> {
        let directory = self.directories[directory].as_ref().map_err(Clone::clone)?;
        let mut errors = directory.errors.clone();
        for file in &directory.files {
            let mut nested_conflicts = Vec::new();
            match self.assemble_file(
                file,
                visit_document,
                conflicts.as_ref().map(|_| &mut nested_conflicts),
            ) {
                Ok(map) => {
                    let merged = if let Some(conflicts) = conflicts.as_deref_mut() {
                        let merged = merge_with_deferred_value(
                            result,
                            file.name.clone(),
                            Value::Object(map),
                            &mut nested_conflicts,
                        );
                        prefix_conflicts(&mut nested_conflicts, &file.name);
                        conflicts.extend(nested_conflicts);
                        merged
                    } else {
                        merge_with_value(result, file.name.clone(), Value::Object(map))
                    };
                    if let Err(failures) = merged {
                        errors.extend(failures);
                    }
                }
                Err(failures) => errors.extend(failures),
            }
        }
        for (name, directory) in &directory.folders {
            // Inline keys and successfully loaded files take precedence over standalone
            // folders. Evaluate this after file merges, using the parent's existing map.
            if !result.contains_key(name) {
                let mut map = ConfigMap::new();
                let mut nested_conflicts = Vec::new();
                match self.assemble_directory(
                    *directory,
                    &mut map,
                    visit_document,
                    conflicts.as_ref().map(|_| &mut nested_conflicts),
                ) {
                    Ok(()) => {
                        result.insert(name.clone(), Value::Object(map));
                        if let Some(conflicts) = conflicts.as_deref_mut() {
                            prefix_conflicts(&mut nested_conflicts, name);
                            conflicts.extend(nested_conflicts);
                        }
                    }
                    Err(failures) => errors.extend(failures),
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn assemble_file(
        &self,
        file: &NamedFile,
        visit_document: &mut impl FnMut(&ConfigMap),
        conflicts: Option<&mut Vec<DeferredConflict>>,
    ) -> Result<ConfigMap, Vec<String>> {
        let mut map = self.document(file.document, visit_document)?;
        if let Some(nested) = file.nested {
            self.assemble_directory(nested, &mut map, visit_document, conflicts)?;
        }
        Ok(map)
    }
}

#[derive(Clone, Default)]
struct DirectoryListing {
    files: Vec<PathBuf>,
    folders: Vec<PathBuf>,
    errors: Vec<String>,
}

/// Filesystem access is confined to snapshot construction. Listings and documents are shared,
/// but directory plans are specific to their parent's inline fields.
#[derive(Default)]
struct InputReader {
    parsed: ParsedInputs,
    documents: HashMap<DocumentOrigin, Option<DocumentId>>,
    listings: HashMap<PathBuf, Result<DirectoryListing, Vec<String>>>,
    active_directories: HashSet<(PathBuf, Vec<String>)>,
}

impl InputReader {
    fn file(&mut self, path: &Path, format: Format) -> Option<(String, DocumentId)> {
        let name = component_name(path).ok()?;
        let origin = DocumentOrigin {
            path: Some(path.to_owned()),
            format,
        };
        if let Some(document) = self.documents.get(&origin) {
            return document.map(|document| (name, document));
        }
        if let Some(input) = open_file(path) {
            let document = ParsedDocument {
                origin,
                value: parse_document(input, format),
            };
            let id = self.parsed.documents.len();
            self.documents.insert(document.origin.clone(), Some(id));
            self.parsed.documents.push(document);
            Some((name, id))
        } else {
            self.documents.insert(origin, None);
            None
        }
    }

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::unnecessary_debug_formatting,
        reason = "Preserve diagnostic text and escaping"
    )]
    fn directory(&mut self, path: &Path, recurse: bool, initial: &ConfigMap) -> DirectoryId {
        let directory = self.parsed.directories.len();
        self.parsed.directories.push(Ok(ParsedDirectory::default()));
        // A revisit with different inline fields can suppress the link that led here.
        // Only reject a repeated ancestor with the same initial field names.
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        let mut keys = initial.keys().cloned().collect::<Vec<_>>();
        keys.sort_unstable();
        let context = (canonical, keys);
        let parsed = if self.active_directories.insert(context.clone()) {
            let parsed = self.read_directory(path, recurse, initial);
            self.active_directories.remove(&context);
            parsed
        } else {
            Err(vec![format!(
                "Configuration directory contains a symbolic-link cycle: {path:?}."
            )])
        };
        self.parsed.directories[directory] = parsed;
        directory
    }

    fn read_directory(
        &mut self,
        path: &Path,
        recurse: bool,
        initial: &ConfigMap,
    ) -> Result<ParsedDirectory, Vec<String>> {
        let listing = self
            .listings
            .entry(path.to_owned())
            .or_insert_with(|| Self::list_directory(path))
            .clone()?;
        let mut directory = ParsedDirectory {
            errors: listing.errors,
            ..ParsedDirectory::default()
        };
        // Interpolation only changes string contents, not keys or value shapes. Replay
        // raw assembly to determine which standalone folders are actually reachable.
        // If interpolation later fails, assembly reports that error without reading a
        // previously hidden folder solely to discover additional secondary failures.
        let mut preview = initial.clone();
        for path in listing.files {
            let Ok(format) = Format::from_path(&path) else {
                continue;
            };
            if let Some((name, document)) = self.file(&path, format) {
                let nested = if recurse {
                    let initial = self.parsed.documents[document].value.as_ref().ok().cloned();
                    path.parent()
                        .map(|parent| parent.join(&name))
                        .filter(|path| path.is_dir() && path.exists())
                        .zip(initial)
                        .map(|(path, initial)| self.directory(&path, true, &initial))
                } else {
                    None
                };
                let file = NamedFile {
                    name,
                    document,
                    nested,
                };
                let mut conflicts = Vec::new();
                if recurse
                    && let Ok(map) =
                        self.parsed
                            .assemble_file(&file, &mut |_| {}, Some(&mut conflicts))
                {
                    // Keep the normal merge's key-removal behavior on failure, too.
                    drop(merge_with_deferred_value(
                        &mut preview,
                        file.name.clone(),
                        Value::Object(map),
                        &mut conflicts,
                    ));
                }
                directory.files.push(file);
            }
        }
        if recurse {
            for path in listing.folders {
                if let Ok(name) = component_name(&path)
                    && !preview.contains_key(&name)
                {
                    let nested = self.directory(&path, true, &ConfigMap::new());
                    let mut map = ConfigMap::new();
                    let mut conflicts = Vec::new();
                    if self
                        .parsed
                        .assemble_directory(nested, &mut map, &mut |_| {}, Some(&mut conflicts))
                        .is_ok()
                    {
                        preview.insert(name.clone(), Value::Object(map));
                    }
                    directory.folders.push((name, nested));
                }
            }
        }
        Ok(directory)
    }

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::unnecessary_debug_formatting,
        reason = "Preserve diagnostic text and escaping"
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
        if matches!(format, Format::Toml | Format::Json)
            && (ENVIRONMENT_VARIABLE_INTERPOLATION_REGEX.is_match(&source)
                || COLLECTOR.is_match(&source))
        {
            errors.push(
                "Configuration is parsed before interpolation. Quote placeholders in \
                 TOML and JSON values; they will be coerced to the field's declared \
                 type after substitution."
                    .to_owned(),
            );
        }
        errors
    })?;
    deserialize_config_value(value)
}

/// Updates a configuration map with the merged values of a named key. Inserts if absent.
fn merge_with_value(res: &mut ConfigMap, name: String, value: Value) -> Result<(), Vec<String>> {
    if let Some(existing) = res.remove(&name) {
        res.insert(name, merge_values(existing, value)?);
    } else {
        res.insert(name, value);
    }
    Ok(())
}

fn merge_with_deferred_value(
    res: &mut ConfigMap,
    name: String,
    value: Value,
    conflicts: &mut Vec<DeferredConflict>,
) -> Result<(), Vec<String>> {
    if let Some(existing) = res.remove(&name) {
        res.insert(name, merge_values_deferred(existing, value, conflicts)?);
    } else {
        res.insert(name, value);
    }
    Ok(())
}

fn prefix_conflicts(conflicts: &mut [DeferredConflict], name: &str) {
    for conflict in conflicts {
        conflict.path.insert(0, name.to_owned());
    }
}

fn resolve_directory_root(
    mut files: ConfigMap,
    mut conflicts: Vec<DeferredConflict>,
    scope: CoercionScope,
) -> Result<ConfigMap, Vec<String>> {
    // Root filenames are assembly boundaries, not configuration field names.
    for conflict in &mut conflicts {
        drop(conflict.path.remove(0));
    }
    if matches!(scope, CoercionScope::SecretBackends) {
        for value in files.values_mut() {
            if let Value::Object(map) = value {
                map.retain(|key, _| key == SECRET_KEY);
            }
        }
        conflicts.retain(|conflict| conflict.path.first().is_some_and(|key| key == SECRET_KEY));
    }
    let maps = files.into_values().filter_map(|value| match value {
        Value::Object(map) => Some(map),
        _ => None,
    });
    let (merged, root_conflicts) = merge_maps_deferred(maps)?;
    conflicts.extend(root_conflicts);
    coerce_merge_conflicts(merged, conflicts)
}

fn resolve_component_conflicts(
    mut map: ConfigMap,
    mut conflicts: Vec<DeferredConflict>,
    hint: ComponentHint,
    scope: CoercionScope,
) -> Result<ConfigMap, Vec<String>> {
    if conflicts.is_empty() {
        return Ok(map);
    }
    if matches!(scope, CoercionScope::SecretBackends) {
        // Match secret discovery's existing projection. All other component conflicts
        // remain deferred until their secret values have been substituted.
        conflicts.retain(|conflict| conflict.path.first().is_some_and(|key| key == SECRET_KEY));
        let projection = map
            .get(SECRET_KEY)
            .map(|value| (SECRET_KEY.to_owned(), value.clone()))
            .into_iter()
            .collect();
        let mut projection = coerce_merge_conflicts(projection, conflicts)?;
        if let Some(backends) = projection.remove(SECRET_KEY) {
            map.insert(SECRET_KEY.to_owned(), backends);
        }
        return Ok(map);
    }

    let field = hint.as_component_field();
    if matches!(hint, ComponentHint::Test) {
        let names: Vec<_> = map.keys().cloned().collect();
        for conflict in &mut conflicts {
            if let Some(index) = names
                .iter()
                .position(|name| conflict.path.first() == Some(name))
            {
                conflict.path[0] = index.to_string();
            }
        }
        prefix_conflicts(&mut conflicts, field);
        let root =
            ConfigMap::from_iter([(field.to_owned(), Value::Array(map.into_values().collect()))]);
        let mut root = coerce_merge_conflicts(root, conflicts)?;
        let values: Vec<Value> =
            deserialize_config_value(root.remove(field).unwrap_or(Value::Null))?;
        Ok(names.into_iter().zip(values).collect())
    } else {
        prefix_conflicts(&mut conflicts, field);
        let root = ConfigMap::from_iter([(field.to_owned(), Value::Object(map))]);
        let mut root = coerce_merge_conflicts(root, conflicts)?;
        deserialize_config_value(root.remove(field).unwrap_or(Value::Null))
    }
}

/// Coerces a root configuration before serde performs authoritative deserialization.
pub(super) fn deserialize_config_map<T: serde::de::DeserializeOwned>(
    map: ConfigMap,
) -> Result<T, Vec<String>> {
    let mut value = Value::Object(map);
    coerce_config(&mut value)?;
    deserialize_config_value(value)
}

pub(super) fn merge_root_config(files: ConfigMap) -> Result<ConfigMap, Vec<String>> {
    resolve_directory_root(files, Vec::new(), CoercionScope::Configuration)
}

fn coerce_merge_conflicts(
    map: ConfigMap,
    conflicts: Vec<DeferredConflict>,
) -> Result<ConfigMap, Vec<String>> {
    // Generate at most one schema, and only when a conflict needs to be checked.
    let mut schema = None;
    resolve_merge_conflicts(map, conflicts, |value| {
        let schema = schema
            .get_or_insert_with(config_schema)
            .as_ref()
            .map_err(Clone::clone)?;
        ValueCoercer::new(schema)
            .coerce(value)
            .map_err(|error| vec![error.to_string()])
    })
}

/// Coerces a namespaced component map using its schema at the root configuration field.
pub(super) fn deserialize_component_map<T: serde::de::DeserializeOwned>(
    map: ConfigMap,
    hint: ComponentHint,
) -> Result<indexmap::IndexMap<crate::config::ComponentKey, T>, Vec<String>> {
    let key = hint.as_component_field();
    let mut value = Value::Object(ConfigMap::from_iter([(key.to_owned(), Value::Object(map))]));
    coerce_config(&mut value)?;
    deserialize_config_value(value[key].take())
}

fn coerce_config(value: &mut Value) -> Result<(), Vec<String>> {
    let schema = config_schema()?;
    ValueCoercer::new(&schema)
        .coerce(value)
        .map_err(|error| vec![error.to_string()])
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
