use crate::{
    buffers::Acker,
    conditions,
    dns::Resolver,
    event::{self, Event, Metric},
    runtime::TaskExecutor,
    shutdown::ShutdownSignal,
    sinks, sources, transforms,
};
use component::ComponentDescription;
use futures01::sync::mpsc;
use indexmap::IndexMap; // IndexMap preserves insertion order, allowing us to output errors in the same order they are present in the file
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::fs::DirBuilder;
use std::{collections::HashMap, path::PathBuf};

pub mod component;
mod validation;
mod vars;
pub mod watcher;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(flatten)]
    pub global: GlobalOptions,
    #[serde(default)]
    pub sources: IndexMap<String, Box<dyn SourceConfig>>,
    #[serde(default)]
    pub sinks: IndexMap<String, SinkOuter>,
    #[serde(default)]
    pub transforms: IndexMap<String, TransformOuter>,
    #[serde(default)]
    pub tests: Vec<TestDefinition>,
}

#[derive(Default, Debug, Deserialize, Serialize)]
pub struct GlobalOptions {
    #[serde(default = "default_data_dir")]
    pub data_dir: Option<PathBuf>,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub log_schema: event::LogSchema,
}

pub fn default_data_dir() -> Option<PathBuf> {
    Some(PathBuf::from("/var/lib/vector/"))
}

#[derive(Debug, Snafu)]
pub enum DataDirError {
    #[snafu(display("data_dir option required, but not given here or globally"))]
    MissingDataDir,
    #[snafu(display("data_dir {:?} does not exist", data_dir))]
    DoesNotExist { data_dir: PathBuf },
    #[snafu(display("data_dir {:?} is not writable", data_dir))]
    NotWritable { data_dir: PathBuf },
    #[snafu(display(
        "Could not create subdirectory {:?} inside of data dir {:?}: {}",
        subdir,
        data_dir,
        source
    ))]
    CouldNotCreate {
        subdir: PathBuf,
        data_dir: PathBuf,
        source: std::io::Error,
    },
}

impl GlobalOptions {
    /// Resolve the `data_dir` option in either the global or local
    /// config, and validate that it exists and is writable.
    pub fn resolve_and_validate_data_dir(
        &self,
        local_data_dir: Option<&PathBuf>,
    ) -> crate::Result<PathBuf> {
        let data_dir = local_data_dir
            .or(self.data_dir.as_ref())
            .ok_or_else(|| DataDirError::MissingDataDir)
            .map_err(Box::new)?
            .to_path_buf();
        if !data_dir.exists() {
            return Err(DataDirError::DoesNotExist { data_dir }.into());
        }
        let readonly = std::fs::metadata(&data_dir)
            .map(|meta| meta.permissions().readonly())
            .unwrap_or(true);
        if readonly {
            return Err(DataDirError::NotWritable { data_dir }.into());
        }
        Ok(data_dir)
    }

    /// Resolve the `data_dir` option using
    /// `resolve_and_validate_data_dir` and then ensure a named
    /// subdirectory exists.
    pub fn resolve_and_make_data_subdir(
        &self,
        local: Option<&PathBuf>,
        subdir: &str,
    ) -> crate::Result<PathBuf> {
        let data_dir = self.resolve_and_validate_data_dir(local)?;

        let mut data_subdir = data_dir.clone();
        data_subdir.push(subdir);

        DirBuilder::new()
            .recursive(true)
            .create(&data_subdir)
            .with_context(|| CouldNotCreate { subdir, data_dir })?;
        Ok(data_subdir)
    }
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum DataType {
    Any,
    Log,
    Metric,
}

#[typetag::serde(tag = "type")]
pub trait SourceConfig: core::fmt::Debug {
    fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<sources::Source>;

    fn output_type(&self) -> DataType;

    fn source_type(&self) -> &'static str;
}

pub type SourceDescription = ComponentDescription<Box<dyn SourceConfig>>;

inventory::collect!(SourceDescription);

#[derive(Deserialize, Serialize, Debug)]
pub struct SinkOuter {
    #[serde(default)]
    pub buffer: crate::buffers::BufferConfig,
    #[serde(default = "healthcheck_default")]
    pub healthcheck: bool,
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Box<dyn SinkConfig>,
}

#[typetag::serde(tag = "type")]
pub trait SinkConfig: core::fmt::Debug {
    fn build(&self, cx: SinkContext) -> crate::Result<(sinks::RouterSink, sinks::Healthcheck)>;

    fn input_type(&self) -> DataType;

    fn sink_type(&self) -> &'static str;
}

#[derive(Debug, Clone)]
pub struct SinkContext {
    pub(super) acker: Acker,
    pub(super) resolver: Resolver,
    pub(super) exec: TaskExecutor,
}

impl SinkContext {
    #[cfg(test)]
    pub fn new_test(exec: TaskExecutor) -> Self {
        Self {
            acker: Acker::Null,
            resolver: Resolver,
            exec,
        }
    }

    pub fn acker(&self) -> Acker {
        self.acker.clone()
    }

    pub fn exec(&self) -> TaskExecutor {
        self.exec.clone()
    }

    pub fn resolver(&self) -> Resolver {
        self.resolver.clone()
    }

    pub fn executor(&self) -> &TaskExecutor {
        &self.exec
    }
}

pub type SinkDescription = ComponentDescription<Box<dyn SinkConfig>>;

inventory::collect!(SinkDescription);

#[derive(Deserialize, Serialize, Debug)]
pub struct TransformOuter {
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Box<dyn TransformConfig>,
}

#[typetag::serde(tag = "type")]
pub trait TransformConfig: core::fmt::Debug {
    fn build(&self, cx: TransformContext) -> crate::Result<Box<dyn transforms::Transform>>;

    fn input_type(&self) -> DataType;

    fn output_type(&self) -> DataType;

    fn transform_type(&self) -> &'static str;

    /// Allows a transform configuration to expand itself into multiple "child"
    /// transformations to replace it. This allows a transform to act as a macro
    /// for various patterns.
    fn expand(&mut self) -> crate::Result<Option<IndexMap<String, Box<dyn TransformConfig>>>> {
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct TransformContext {
    pub(super) exec: TaskExecutor,
    pub(super) resolver: Resolver,
}

impl TransformContext {
    pub fn new_test(exec: TaskExecutor) -> Self {
        Self {
            resolver: Resolver,
            exec,
        }
    }

    pub fn executor(&self) -> &TaskExecutor {
        &self.exec
    }

    pub fn resolver(&self) -> Resolver {
        self.resolver.clone()
    }
}

pub type TransformDescription = ComponentDescription<Box<dyn TransformConfig>>;

inventory::collect!(TransformDescription);

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestDefinition {
    pub name: String,
    pub input: Option<TestInput>,
    #[serde(default)]
    pub inputs: Vec<TestInput>,
    #[serde(default)]
    pub outputs: Vec<TestOutput>,
    #[serde(default)]
    pub no_outputs_from: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
pub enum TestInputValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestInput {
    pub insert_at: String,
    #[serde(default = "default_test_input_type", rename = "type")]
    pub type_str: String,
    pub value: Option<String>,
    pub log_fields: Option<IndexMap<String, TestInputValue>>,
    pub metric: Option<Metric>,
}

fn default_test_input_type() -> String {
    "raw".to_string()
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestOutput {
    pub extract_from: String,
    pub conditions: Option<Vec<TestCondition>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum TestCondition {
    Embedded(Box<dyn conditions::ConditionConfig>),
    NoTypeEmbedded(conditions::CheckFieldsConfig),
    String(String),
}

// Helper methods for programming construction during tests
impl Config {
    pub fn empty() -> Self {
        Self {
            global: GlobalOptions {
                data_dir: None,
                log_schema: event::LogSchema::default(),
            },
            sources: IndexMap::new(),
            sinks: IndexMap::new(),
            transforms: IndexMap::new(),
            tests: Vec::new(),
        }
    }

    pub fn add_source<S: SourceConfig + 'static>(&mut self, name: &str, source: S) {
        self.sources.insert(name.to_string(), Box::new(source));
    }

    pub fn add_sink<S: SinkConfig + 'static>(&mut self, name: &str, inputs: &[&str], sink: S) {
        let inputs = inputs.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        let sink = SinkOuter {
            buffer: Default::default(),
            healthcheck: true,
            inner: Box::new(sink),
            inputs,
        };

        self.sinks.insert(name.to_string(), sink);
    }

    pub fn add_transform<T: TransformConfig + 'static>(
        &mut self,
        name: &str,
        inputs: &[&str],
        transform: T,
    ) {
        let inputs = inputs.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        let transform = TransformOuter {
            inner: Box::new(transform),
            inputs,
        };

        self.transforms.insert(name.to_string(), transform);
    }

    /// Some component configs can act like macros and expand themselves into
    /// multiple replacement configs. Returns a map of components to their
    /// expanded child names.
    pub fn expand_macros(&mut self) -> Result<IndexMap<String, Vec<String>>, Vec<String>> {
        let mut expanded_transforms = IndexMap::new();
        let mut expansions = IndexMap::new();
        let mut errors = Vec::new();

        while let Some((k, mut t)) = self.transforms.pop() {
            if let Some(expanded) = match t.inner.expand() {
                Ok(e) => e,
                Err(err) => {
                    errors.push(format!("failed to expand transform '{}': {}", k, err));
                    continue;
                }
            } {
                let mut children = Vec::new();
                for (name, child) in expanded {
                    let full_name = format!("{}.{}", k, name);
                    expanded_transforms.insert(
                        full_name.clone(),
                        TransformOuter {
                            inputs: t.inputs.clone(),
                            inner: child,
                        },
                    );
                    children.push(full_name);
                }
                expansions.insert(k.clone(), children);
            } else {
                expanded_transforms.insert(k, t);
            }
        }
        self.transforms = expanded_transforms;

        if !errors.is_empty() {
            Err(errors)
        } else {
            Ok(expansions)
        }
    }

    pub fn load(mut input: impl std::io::Read) -> Result<Self, Vec<String>> {
        let mut source_string = String::new();
        input
            .read_to_string(&mut source_string)
            .map_err(|e| vec![e.to_string()])?;

        let mut vars = std::env::vars().collect::<HashMap<_, _>>();
        if !vars.contains_key("HOSTNAME") {
            if let Some(hostname) = hostname::get_hostname() {
                vars.insert("HOSTNAME".into(), hostname);
            }
        }
        let with_vars = vars::interpolate(&source_string, &vars);

        toml::from_str(&with_vars).map_err(|e| vec![e.to_string()])
    }

    pub fn append(&mut self, with: Self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.global.data_dir.is_none() || self.global.data_dir == default_data_dir() {
            self.global.data_dir = with.global.data_dir;
        } else if with.global.data_dir != default_data_dir()
            && self.global.data_dir != with.global.data_dir
        {
            // If two configs both set 'data_dir' and have conflicting values
            // we consider this an error.
            errors.push("conflicting values for 'data_dir' found".to_owned());
        }

        // If the user has multiple config files, we must *merge* log schemas until we meet a
        // conflict, then we are allowed to error.
        let default_schema = event::LogSchema::default();
        if with.global.log_schema != default_schema {
            // If the set value is the default, override it. If it's already overridden, error.
            if self.global.log_schema.host_key() != default_schema.host_key()
                && self.global.log_schema.host_key() != with.global.log_schema.host_key()
            {
                errors.push("conflicting values for 'log_schema.host_key' found".to_owned());
            } else {
                self.global
                    .log_schema
                    .set_host_key(with.global.log_schema.host_key().clone());
            }
            if self.global.log_schema.message_key() != default_schema.message_key()
                && self.global.log_schema.message_key() != with.global.log_schema.message_key()
            {
                errors.push("conflicting values for 'log_schema.message_key' found".to_owned());
            } else {
                self.global
                    .log_schema
                    .set_message_key(with.global.log_schema.message_key().clone());
            }
            if self.global.log_schema.timestamp_key() != default_schema.timestamp_key()
                && self.global.log_schema.timestamp_key() != with.global.log_schema.timestamp_key()
            {
                errors.push("conflicting values for 'log_schema.timestamp_key' found".to_owned());
            } else {
                self.global
                    .log_schema
                    .set_timestamp_key(with.global.log_schema.timestamp_key().clone());
            }
        }

        with.sources.keys().for_each(|k| {
            if self.sources.contains_key(k) {
                errors.push(format!("duplicate source name found: {}", k));
            }
        });
        with.sinks.keys().for_each(|k| {
            if self.sinks.contains_key(k) {
                errors.push(format!("duplicate sink name found: {}", k));
            }
        });
        with.transforms.keys().for_each(|k| {
            if self.transforms.contains_key(k) {
                errors.push(format!("duplicate transform name found: {}", k));
            }
        });
        with.tests.iter().for_each(|wt| {
            if self.tests.iter().any(|t| t.name == wt.name) {
                errors.push(format!("duplicate test name found: {}", wt.name));
            }
        });
        if !errors.is_empty() {
            return Err(errors);
        }

        self.sources.extend(with.sources);
        self.sinks.extend(with.sinks);
        self.transforms.extend(with.transforms);
        self.tests.extend(with.tests);

        Ok(())
    }

    pub fn typecheck(&self) -> Result<(), Vec<String>> {
        validation::typecheck(self)
    }
}

impl Clone for Config {
    fn clone(&self) -> Self {
        // This is a hack around the issue of cloning
        // trait objects. So instead to clone the config
        // we first serialize it into json, then back from
        // json. Originally we used toml here but toml does not
        // support serializing `None`.
        let json = serde_json::to_vec(self).unwrap();
        serde_json::from_slice(&json[..]).unwrap()
    }
}

fn healthcheck_default() -> bool {
    true
}

#[cfg(all(
    test,
    feature = "sources-file",
    feature = "sinks-console",
    feature = "transforms-json_parser"
))]
mod test {
    use super::Config;
    use std::path::PathBuf;

    #[test]
    fn default_data_dir() {
        let config: Config = toml::from_str(
            r#"
      [sources.in]
      type = "file"
      include = ["/var/log/messages"]

      [sinks.out]
      type = "console"
      inputs = ["in"]
      encoding = "json"
      "#,
        )
        .unwrap();

        assert_eq!(
            Some(PathBuf::from("/var/lib/vector")),
            config.global.data_dir
        )
    }

    #[test]
    fn default_schema() {
        let config: Config = toml::from_str(
            r#"
      [sources.in]
      type = "file"
      include = ["/var/log/messages"]

      [sinks.out]
      type = "console"
      inputs = ["in"]
      encoding = "json"
      "#,
        )
        .unwrap();

        assert_eq!("host", config.global.log_schema.host_key().to_string());
        assert_eq!(
            "message",
            config.global.log_schema.message_key().to_string()
        );
        assert_eq!(
            "timestamp",
            config.global.log_schema.timestamp_key().to_string()
        );
    }

    #[test]
    fn custom_schema() {
        let config: Config = toml::from_str(
            r#"
      [log_schema]
      host_key = "this"
      message_key = "that"
      timestamp_key = "then"

      [sources.in]
      type = "file"
      include = ["/var/log/messages"]

      [sinks.out]
      type = "console"
      inputs = ["in"]
      encoding = "json"
      "#,
        )
        .unwrap();

        assert_eq!("this", config.global.log_schema.host_key().to_string());
        assert_eq!("that", config.global.log_schema.message_key().to_string());
        assert_eq!("then", config.global.log_schema.timestamp_key().to_string());
    }

    #[test]
    fn config_append() {
        let mut config: Config = toml::from_str(
            r#"
      [sources.in]
      type = "file"
      include = ["/var/log/messages"]

      [sinks.out]
      type = "console"
      inputs = ["in"]
      encoding = "json"
      "#,
        )
        .unwrap();

        assert_eq!(
            config.append(
                toml::from_str(
                    r#"
        data_dir = "/foobar"

        [transforms.foo]
        type = "json_parser"
        inputs = [ "in" ]

        [[tests]]
        name = "check_simple_log"
        [tests.input]
        insert_at = "foo"
        type = "raw"
        value = "2019-11-28T12:00:00+00:00 info Sorry, I'm busy this week Cecil"
        [[tests.outputs]]
        extract_from = "foo"
        [[tests.outputs.conditions]]
        type = "check_fields"
        "message.equals" = "Sorry, I'm busy this week Cecil"
            "#,
                )
                .unwrap()
            ),
            Ok(())
        );

        assert_eq!(Some(PathBuf::from("/foobar")), config.global.data_dir);
        assert!(config.sources.contains_key("in"));
        assert!(config.sinks.contains_key("out"));
        assert!(config.transforms.contains_key("foo"));
        assert_eq!(config.tests.len(), 1);
    }

    #[test]
    fn config_append_collisions() {
        let mut config: Config = toml::from_str(
            r#"
      [sources.in]
      type = "file"
      include = ["/var/log/messages"]

      [sinks.out]
      type = "console"
      inputs = ["in"]
      encoding = "json"
      "#,
        )
        .unwrap();

        assert_eq!(
            config.append(
                toml::from_str(
                    r#"
        [sources.in]
        type = "file"
        include = ["/var/log/messages"]

        [transforms.foo]
        type = "json_parser"
        inputs = [ "in" ]

        [sinks.out]
        type = "console"
        inputs = ["in"]
        encoding = "json"
            "#,
                )
                .unwrap()
            ),
            Err(vec![
                "duplicate source name found: in".into(),
                "duplicate sink name found: out".into(),
            ])
        );
    }
}
