use crate::topology::config::{
    component::ExampleError, GlobalOptions, SinkDescription, SourceDescription,
    TransformDescription,
};
use indexmap::IndexMap;
use serde::Serialize;
use std::collections::BTreeMap;
use structopt::StructOpt;
use toml::Value;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Generate expression, e.g. 'stdin|json_parser,add_fields|console'
    ///
    /// Three comma-separated lists of sources, transforms and sinks, separated
    /// by pipes. If subsequent component types are not needed then their pipes
    /// can be omitted from the expression.
    ///
    /// For example:
    ///
    /// `|json_parser` prints a `json_parser` transform.
    ///
    /// `||file,http` prints a `file` and `http` sink.
    ///
    /// `stdin||http` prints a `stdin` source and an `http` sink.
    ///
    /// Vector makes a best attempt at constructing a sensible topology. The
    /// first transform generated will consume from all sources and subsequent
    /// transforms will consume from their predecessor. All sinks will consume
    /// from the last transform or, if none are specified, from all sources. It
    /// is then up to you to restructure the `inputs` of each component to build
    /// the topology you need.
    ///
    /// Generated components are given incremental names (`source1`, `source2`,
    /// etc) which should be replaced in order to provide better context.
    expression: String,
}

#[derive(Serialize)]
pub struct SinkOuter {
    pub healthcheck: bool,
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Value,
    pub buffer: crate::buffers::BufferConfig,
}

#[derive(Serialize)]
pub struct TransformOuter {
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Value,
}

#[derive(Serialize, Default)]
pub struct Config {
    #[serde(flatten)]
    pub global: GlobalOptions,
    pub sources: Option<IndexMap<String, Value>>,
    pub transforms: Option<IndexMap<String, TransformOuter>>,
    pub sinks: Option<IndexMap<String, SinkOuter>>,
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let components: Vec<Vec<_>> = opts
        .expression
        .split('|')
        .map(|s| {
            s.split(',')
                .map(|s| s.trim())
                .filter(|s| s.len() > 0)
                .collect()
        })
        .collect();

    let mut config = Config::default();
    config.global.data_dir = crate::topology::config::default_data_dir();

    let mut errs = Vec::new();

    let mut source_names = Vec::new();
    if let Some(source_types) = components.get(0) {
        let mut sources = IndexMap::new();

        for (i, source_type) in source_types.iter().enumerate() {
            let name = format!("source{}", i);
            source_names.push(name.clone());

            let mut example = match SourceDescription::example(source_type) {
                Ok(example) => example,
                Err(err) => {
                    if err != ExampleError::MissingExample {
                        errs.push(err);
                    }
                    Value::Table(BTreeMap::new())
                }
            };
            example
                .as_table_mut()
                .expect("examples are always tables")
                .insert("type".into(), source_type.to_owned().into());

            sources.insert(name, example);
        }

        if sources.len() > 0 {
            config.sources = Some(sources);
        }
    }

    let mut transform_names = Vec::new();
    if let Some(transform_types) = components.get(1) {
        let mut transforms = IndexMap::new();

        for (i, transform_type) in transform_types.iter().enumerate() {
            let name = format!("transform{}", i);
            transform_names.push(name.clone());

            let targets = if i == 0 {
                source_names.clone()
            } else {
                vec![transform_names
                    .get(i - 1)
                    .unwrap_or(&"TODO".to_owned())
                    .to_owned()]
            };

            let mut example = match TransformDescription::example(transform_type) {
                Ok(example) => example,
                Err(err) => {
                    if err != ExampleError::MissingExample {
                        errs.push(err);
                    }
                    Value::Table(BTreeMap::new())
                }
            };
            example
                .as_table_mut()
                .expect("examples are always tables")
                .insert("type".into(), transform_type.to_owned().into());

            transforms.insert(
                name,
                TransformOuter {
                    inputs: targets,
                    inner: example,
                },
            );
        }

        if transforms.len() > 0 {
            config.transforms = Some(transforms);
        }
    }

    if let Some(sink_types) = components.get(2) {
        let mut sinks = IndexMap::new();

        for (i, sink_type) in sink_types.iter().enumerate() {
            let mut example = match SinkDescription::example(sink_type) {
                Ok(example) => example,
                Err(err) => {
                    if err != ExampleError::MissingExample {
                        errs.push(err);
                    }
                    Value::Table(BTreeMap::new())
                }
            };
            example
                .as_table_mut()
                .expect("examples are always tables")
                .insert("type".into(), sink_type.to_owned().into());

            sinks.insert(
                format!("sink{}", i),
                SinkOuter {
                    inputs: transform_names
                        .last()
                        .map(|s| vec![s.to_owned()])
                        .or_else(|| {
                            if source_names.len() > 0 {
                                Some(source_names.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or(vec!["TODO".to_owned()]),
                    buffer: crate::buffers::BufferConfig::default(),
                    healthcheck: true,
                    inner: example,
                },
            );
        }

        if sinks.len() > 0 {
            config.sinks = Some(sinks);
        }
    }

    if errs.len() > 0 {
        errs.iter().for_each(|e| eprintln!("Generate error: {}", e));
        return exitcode::CONFIG;
    }

    match toml::to_string_pretty(&config) {
        Ok(s) => {
            println!("{}", s);
            exitcode::OK
        }
        Err(e) => {
            eprintln!("Failed to generate config: {}.", e);
            exitcode::SOFTWARE
        }
    }
}
