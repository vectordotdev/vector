use crate::{event::Event, sinks, sources, transforms};
use futures::sync::mpsc;
use indexmap::IndexMap; // IndexMap preserves insertion order, allowing us to output errors in the same order they are present in the file
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

mod validation;
mod vars;

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    pub sources: IndexMap<String, Box<dyn SourceConfig>>,
    pub sinks: IndexMap<String, SinkOuter>,
    #[serde(default)]
    pub transforms: IndexMap<String, TransformOuter>,
}

#[derive(Default)]
pub struct GlobalOptions {
    pub data_dir: Option<PathBuf>,
}

impl GlobalOptions {
    pub fn from(config: &Config) -> Self {
        Self {
            data_dir: config.data_dir.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
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
        out: mpsc::Sender<Event>,
    ) -> Result<sources::Source, String>;

    fn output_type(&self) -> DataType;
}

#[derive(Deserialize, Serialize, Debug)]
pub struct SinkOuter {
    #[serde(default)]
    pub buffer: crate::buffers::BufferConfig,
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Box<SinkConfig>,
}

#[typetag::serde(tag = "type")]
pub trait SinkConfig: core::fmt::Debug {
    fn build(
        &self,
        acker: crate::buffers::Acker,
    ) -> Result<(sinks::RouterSink, sinks::Healthcheck), String>;

    fn input_type(&self) -> DataType;
}

#[derive(Deserialize, Serialize, Debug)]
pub struct TransformOuter {
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Box<TransformConfig>,
}

#[typetag::serde(tag = "type")]
pub trait TransformConfig: core::fmt::Debug {
    fn build(&self) -> Result<Box<dyn transforms::Transform>, String>;

    fn input_type(&self) -> DataType;

    fn output_type(&self) -> DataType;
}

// Helper methods for programming construction during tests
impl Config {
    pub fn empty() -> Self {
        Self {
            data_dir: None,
            sources: IndexMap::new(),
            sinks: IndexMap::new(),
            transforms: IndexMap::new(),
        }
    }

    pub fn add_source<S: SourceConfig + 'static>(&mut self, name: &str, source: S) {
        self.sources.insert(name.to_string(), Box::new(source));
    }

    pub fn add_sink<S: SinkConfig + 'static>(&mut self, name: &str, inputs: &[&str], sink: S) {
        let inputs = inputs.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        let sink = SinkOuter {
            buffer: Default::default(),
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

    pub fn contains_cycle(&self) -> bool {
        validation::contains_cycle(self)
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
