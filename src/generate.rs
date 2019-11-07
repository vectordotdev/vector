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
    pub sources: IndexMap<String, Value>,
    pub transforms: IndexMap<String, TransformOuter>,
    pub sinks: IndexMap<String, SinkOuter>,
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let components: Vec<Vec<String>> = opts
        .expression
        .split('|')
        .map(|s| {
            s.to_owned()
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| s.len() > 0)
                .collect()
        })
        .collect();

    let mut config = Config::default();
    config.global.data_dir = crate::topology::config::default_data_dir();

    let mut errs = Vec::new();

    let mut i = 0;
    let mut source_names = Vec::new();
    components
        .get(0)
        .unwrap_or(&Vec::new())
        .iter()
        .for_each(|c| {
            i += 1;
            let name = format!("source{}", i);
            source_names.push(name.clone());
            config.sources.insert(name, {
                let mut d = SourceDescription::example(c)
                    .map_err(|e| {
                        match e {
                            ExampleError::MissingExample => {}
                            _ => errs.push(e.clone()),
                        }
                        e
                    })
                    .unwrap_or(Value::Table(BTreeMap::new()));
                d.as_table_mut().map(|s| {
                    s.insert("type".to_owned(), c.to_owned().into());
                    s
                });
                d
            });
        });

    i = 0;
    let mut transform_names = Vec::new();
    components
        .get(1)
        .unwrap_or(&Vec::new())
        .iter()
        .for_each(|c| {
            i += 1;
            let name = format!("transform{}", i);
            transform_names.push(name.clone());
            let targets = if i == 1 {
                source_names.clone()
            } else {
                vec![transform_names
                    .get(i - 2)
                    .unwrap_or(&"TODO".to_owned())
                    .to_owned()]
            };
            config.transforms.insert(
                name,
                TransformOuter {
                    inputs: targets,
                    inner: {
                        let mut d = TransformDescription::example(c)
                            .map_err(|e| {
                                match e {
                                    ExampleError::MissingExample => {}
                                    _ => errs.push(e.clone()),
                                }
                                e
                            })
                            .unwrap_or(Value::Table(BTreeMap::new()));
                        d.as_table_mut().map(|s| {
                            s.insert("type".to_owned(), c.to_owned().into());
                            s
                        });
                        d
                    },
                },
            );
        });

    i = 0;
    components
        .get(2)
        .unwrap_or(&Vec::new())
        .iter()
        .for_each(|c| {
            i += 1;
            config.sinks.insert(
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
                    inner: {
                        let mut d = SinkDescription::example(c)
                            .map_err(|e| {
                                match e {
                                    ExampleError::MissingExample => {}
                                    _ => errs.push(e.clone()),
                                }
                                e
                            })
                            .unwrap_or(Value::Table(BTreeMap::new()));
                        d.as_table_mut().map(|s| {
                            s.insert("type".to_owned(), c.to_owned().into());
                            s
                        });
                        d
                    },
                },
            );
        });

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
