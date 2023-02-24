#![allow(missing_docs)]
use std::{
    fs::{create_dir_all, File},
    io::Write,
    path::{Path, PathBuf},
};

use clap::Parser;
use colored::*;
use indexmap::IndexMap;
use serde::Serialize;
use toml::{map::Map, Value};
use vector_config::component::{
    ExampleError, SinkDescription, SourceDescription, TransformDescription,
};
use vector_core::{buffers::BufferConfig, config::GlobalOptions, default_data_dir};

use crate::config::SinkHealthcheckOptions;

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// Whether to skip the generation of global fields.
    #[arg(short, long)]
    fragment: bool,

    /// Generate expression, e.g. 'stdin/remap,filter/console'
    ///
    /// Three comma-separated lists of sources, transforms and sinks, divided by
    /// forward slashes. If subsequent component types are not needed then
    /// their dividers can be omitted from the expression.
    ///
    /// For example:
    ///
    /// `/filter` prints a `filter` transform.
    ///
    /// `//file,http` prints a `file` and `http` sink.
    ///
    /// `stdin//http` prints a `stdin` source and an `http` sink.
    ///
    /// Generated components are given incremental names (`source1`, `source2`,
    /// etc) which should be replaced in order to provide better context. You
    /// can optionally specify the names of components by prefixing them with
    /// `<name>:`, e.g.:
    ///
    /// `foo:stdin/bar:test_basic/baz:http` prints a `stdin` source called
    /// `foo`, a `test_basic` transform called `bar`, and an `http` sink
    /// called `baz`.
    ///
    /// Vector makes a best attempt at constructing a sensible topology. The
    /// first transform generated will consume from all sources and subsequent
    /// transforms will consume from their predecessor. All sinks will consume
    /// from the last transform or, if none are specified, from all sources. It
    /// is then up to you to restructure the `inputs` of each component to build
    /// the topology you need.
    expression: String,

    /// Generate config as a file
    #[arg(long)]
    file: Option<PathBuf>,
}

#[derive(Serialize)]
pub struct SinkOuter {
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Value,
    pub healthcheck: SinkHealthcheckOptions,
    pub buffer: BufferConfig,
}

#[derive(Serialize)]
pub struct TransformOuter {
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Value,
}

#[derive(Serialize, Default)]
pub struct Config {
    pub sources: Option<IndexMap<String, Value>>,
    pub transforms: Option<IndexMap<String, TransformOuter>>,
    pub sinks: Option<IndexMap<String, SinkOuter>>,
}

/// Controls how the resulting transform topology is wired up. This is not
/// user-configurable.
pub(crate) enum TransformInputsStrategy {
    /// Default.
    ///
    /// The first transform generated will consume from all sources and
    /// subsequent transforms will consume from their predecessor.
    Auto,
    /// Used for property testing `vector config`.
    ///
    /// All transforms use a list of all sources as inputs.
    #[cfg(test)]
    #[allow(dead_code)]
    All,
}

pub(crate) fn generate_example(
    include_globals: bool,
    expression: &str,
    file: &Option<PathBuf>,
    transform_inputs_strategy: TransformInputsStrategy,
) -> Result<String, Vec<String>> {
    let components: Vec<Vec<_>> = expression
        .split(|c| c == '|' || c == '/')
        .map(|s| {
            s.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .collect();

    let globals = GlobalOptions {
        data_dir: default_data_dir(),
        ..Default::default()
    };
    let mut config = Config::default();

    let mut errs = Vec::new();

    let mut source_names = Vec::new();
    if let Some(source_types) = components.get(0) {
        let mut sources = IndexMap::new();

        for (i, source_expr) in source_types.iter().enumerate() {
            let (name, source_type) = if let Some(c_index) = source_expr.find(':') {
                if c_index == 0 {
                    errs.push(format!(
                        "failed to generate source '{}': empty name is not allowed",
                        source_expr
                    ));
                    continue;
                }
                let mut chopped_expr = source_expr.clone();
                (
                    chopped_expr.drain(..c_index).collect(),
                    chopped_expr.drain(1..).collect(),
                )
            } else {
                (format!("source{}", i), source_expr.clone())
            };
            source_names.push(name.clone());

            let mut example = match SourceDescription::example(&source_type) {
                Ok(example) => example,
                Err(err) => {
                    if err != ExampleError::MissingExample {
                        errs.push(format!(
                            "failed to generate source '{}': {}",
                            source_type, err
                        ));
                    }
                    Value::Table(Map::new())
                }
            };
            example
                .as_table_mut()
                .expect("examples are always tables")
                .insert("type".into(), source_type.to_owned().into());

            sources.insert(name, example);
        }

        if !sources.is_empty() {
            config.sources = Some(sources);
        }
    }

    let mut transform_names = Vec::new();
    if let Some(transform_types) = components.get(1) {
        let mut transforms = IndexMap::new();

        for (i, transform_expr) in transform_types.iter().enumerate() {
            let (name, transform_type) = if let Some(c_index) = transform_expr.find(':') {
                if c_index == 0 {
                    errs.push(format!(
                        "failed to generate transform '{}': empty name is not allowed",
                        transform_expr
                    ));
                    continue;
                }
                let mut chopped_expr = transform_expr.clone();
                (
                    chopped_expr.drain(..c_index).collect(),
                    chopped_expr.drain(1..).collect(),
                )
            } else {
                (format!("transform{}", i), transform_expr.clone())
            };
            transform_names.push(name.clone());

            let targets = match transform_inputs_strategy {
                TransformInputsStrategy::Auto => {
                    if i == 0 {
                        source_names.clone()
                    } else {
                        vec![transform_names
                            .get(i - 1)
                            .unwrap_or(&"component-id".to_owned())
                            .to_owned()]
                    }
                }
                #[cfg(test)]
                TransformInputsStrategy::All => source_names.clone(),
            };

            let mut example = match TransformDescription::example(&transform_type) {
                Ok(example) => example,
                Err(err) => {
                    if err != ExampleError::MissingExample {
                        errs.push(format!(
                            "failed to generate transform '{}': {}",
                            transform_type, err
                        ));
                    }
                    Value::Table(Map::new())
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

        if !transforms.is_empty() {
            config.transforms = Some(transforms);
        }
    }

    if let Some(sink_types) = components.get(2) {
        let mut sinks = IndexMap::new();

        for (i, sink_expr) in sink_types.iter().enumerate() {
            let (name, sink_type) = if let Some(c_index) = sink_expr.find(':') {
                if c_index == 0 {
                    errs.push(format!(
                        "failed to generate sink '{}': empty name is not allowed",
                        sink_expr
                    ));
                    continue;
                }
                let mut chopped_expr = sink_expr.clone();
                (
                    chopped_expr.drain(..c_index).collect(),
                    chopped_expr.drain(1..).collect(),
                )
            } else {
                (format!("sink{}", i), sink_expr.clone())
            };

            let mut example = match SinkDescription::example(&sink_type) {
                Ok(example) => example,
                Err(err) => {
                    if err != ExampleError::MissingExample {
                        errs.push(format!("failed to generate sink '{}': {}", sink_type, err));
                    }
                    Value::Table(Map::new())
                }
            };
            example
                .as_table_mut()
                .expect("examples are always tables")
                .insert("type".into(), sink_type.to_owned().into());

            sinks.insert(
                name,
                SinkOuter {
                    inputs: transform_names
                        .last()
                        .map(|s| vec![s.to_owned()])
                        .or_else(|| {
                            if !source_names.is_empty() {
                                Some(source_names.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| vec!["component-id".to_owned()]),
                    buffer: BufferConfig::default(),
                    healthcheck: SinkHealthcheckOptions::default(),
                    inner: example,
                },
            );
        }

        if !sinks.is_empty() {
            config.sinks = Some(sinks);
        }
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    let mut builder = if include_globals {
        match toml::to_string(&globals) {
            Ok(s) => s,
            Err(err) => {
                errs.push(format!("failed to marshal globals: {}", err));
                return Err(errs);
            }
        }
    } else {
        String::new()
    };
    if let Some(sources) = config.sources {
        match toml::to_string(&{
            Config {
                sources: Some(sources),
                ..Default::default()
            }
        }) {
            Ok(v) => builder = [builder, v].join("\n"),
            Err(e) => errs.push(format!("failed to marshal sources: {}", e)),
        }
    }
    if let Some(transforms) = config.transforms {
        match toml::to_string(&{
            Config {
                transforms: Some(transforms),
                ..Default::default()
            }
        }) {
            Ok(v) => builder = [builder, v].join("\n"),
            Err(e) => errs.push(format!("failed to marshal transforms: {}", e)),
        }
    }
    if let Some(sinks) = config.sinks {
        match toml::to_string(&{
            Config {
                sinks: Some(sinks),
                ..Default::default()
            }
        }) {
            Ok(v) => builder = [builder, v].join("\n"),
            Err(e) => errs.push(format!("failed to marshal sinks: {}", e)),
        }
    }

    if file.is_some() {
        #[allow(clippy::print_stdout)]
        match write_config(file.as_ref().unwrap(), &builder) {
            Ok(_) => {
                println!(
                    "Config file written to {:?}",
                    &file.as_ref().unwrap().join("\n")
                )
            }
            Err(e) => errs.push(format!("failed to write to file: {}", e)),
        };
    };

    if !errs.is_empty() {
        Err(errs)
    } else {
        Ok(builder)
    }
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    match generate_example(
        !opts.fragment,
        &opts.expression,
        &opts.file,
        TransformInputsStrategy::Auto,
    ) {
        Ok(s) => {
            #[allow(clippy::print_stdout)]
            {
                println!("{}", s);
            }
            exitcode::OK
        }
        Err(errs) => {
            #[allow(clippy::print_stderr)]
            {
                errs.iter().for_each(|e| eprintln!("{}", e.red()));
            }
            exitcode::SOFTWARE
        }
    }
}

fn write_config(filepath: &Path, body: &str) -> Result<(), crate::Error> {
    if filepath.exists() {
        // If the file exists, we don't want to overwrite, that's just rude.
        Err(format!("{:?} already exists", &filepath).into())
    } else {
        if let Some(directory) = filepath.parent() {
            create_dir_all(directory)?;
        }
        File::create(filepath)
            .and_then(|mut file| file.write_all(body.as_bytes()))
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_all() {
        let mut errors = Vec::new();

        for name in SourceDescription::types() {
            let param = format!("{}//", name);
            let cfg = generate_example(true, &param, &None, TransformInputsStrategy::Auto).unwrap();
            if let Err(error) = toml::from_str::<crate::config::ConfigBuilder>(&cfg) {
                errors.push((param, error));
            }
        }

        for name in TransformDescription::types() {
            let param = format!("/{}/", name);
            let cfg = generate_example(true, &param, &None, TransformInputsStrategy::Auto).unwrap();
            if let Err(error) = toml::from_str::<crate::config::ConfigBuilder>(&cfg) {
                errors.push((param, error));
            }
        }

        for name in SinkDescription::types() {
            let param = format!("//{}", name);
            let cfg = generate_example(true, &param, &None, TransformInputsStrategy::Auto).unwrap();
            if let Err(error) = toml::from_str::<crate::config::ConfigBuilder>(&cfg) {
                errors.push((param, error));
            }
        }

        for (component, error) in &errors {
            #[allow(clippy::print_stdout)]
            {
                println!("{:?} : {}", component, error);
            }
        }
        assert!(errors.is_empty());
    }

    #[cfg(all(feature = "sources-stdin", feature = "sinks-console"))]
    #[test]
    fn generate_configfile() {
        use std::fs;

        use tempfile::tempdir;

        let tempdir = tempdir().expect("Unable to create tempdir for config");
        let filepath = tempdir.path().join("./config.example.toml");
        let cfg = generate_example(
            true,
            "stdin/test_basic/console",
            &Some(filepath.clone()),
            TransformInputsStrategy::Auto,
        );
        let filecontents = fs::read_to_string(
            fs::canonicalize(&filepath).expect("Could not return canonicalized filepath"),
        )
        .expect("Could not read config file");
        fs::remove_file(filepath).expect("Could not cleanup config file!");
        assert_eq!(cfg.unwrap(), filecontents)
    }

    #[cfg(all(feature = "sources-stdin", feature = "sinks-console"))]
    #[test]
    fn generate_basic() {
        assert_eq!(
            generate_example(
                true,
                "stdin/test_basic/console",
                &None,
                TransformInputsStrategy::Auto
            ),
            Ok(indoc::indoc! {r#"data_dir = "/var/lib/vector/"

                [sources.source0]
                max_length = 102400
                type = "stdin"

                [sources.source0.decoding]
                codec = "bytes"

                [transforms.transform0]
                inputs = ["source0"]
                increase = 0.0
                suffix = ""
                type = "test_basic"

                [sinks.sink0]
                inputs = ["transform0"]
                target = "stdout"
                type = "console"

                [sinks.sink0.encoding]
                codec = "json"

                [sinks.sink0.healthcheck]
                enabled = true

                [sinks.sink0.buffer]
                type = "memory"
                max_events = 500
                when_full = "block"
            "#}
            .to_string())
        );

        assert_eq!(
            generate_example(
                true,
                "stdin|test_basic|console",
                &None,
                TransformInputsStrategy::Auto
            ),
            Ok(indoc::indoc! {r#"data_dir = "/var/lib/vector/"

                [sources.source0]
                max_length = 102400
                type = "stdin"

                [sources.source0.decoding]
                codec = "bytes"

                [transforms.transform0]
                inputs = ["source0"]
                increase = 0.0
                suffix = ""
                type = "test_basic"

                [sinks.sink0]
                inputs = ["transform0"]
                target = "stdout"
                type = "console"

                [sinks.sink0.encoding]
                codec = "json"

                [sinks.sink0.healthcheck]
                enabled = true

                [sinks.sink0.buffer]
                type = "memory"
                max_events = 500
                when_full = "block"
            "#}
            .to_string())
        );

        assert_eq!(
            generate_example(true, "stdin//console", &None, TransformInputsStrategy::Auto),
            Ok(indoc::indoc! {r#"data_dir = "/var/lib/vector/"

                [sources.source0]
                max_length = 102400
                type = "stdin"

                [sources.source0.decoding]
                codec = "bytes"

                [sinks.sink0]
                inputs = ["source0"]
                target = "stdout"
                type = "console"

                [sinks.sink0.encoding]
                codec = "json"

                [sinks.sink0.healthcheck]
                enabled = true

                [sinks.sink0.buffer]
                type = "memory"
                max_events = 500
                when_full = "block"
            "#}
            .to_string())
        );

        assert_eq!(
            generate_example(true, "//console", &None, TransformInputsStrategy::Auto),
            Ok(indoc::indoc! {r#"data_dir = "/var/lib/vector/"

                [sinks.sink0]
                inputs = ["component-id"]
                target = "stdout"
                type = "console"

                [sinks.sink0.encoding]
                codec = "json"

                [sinks.sink0.healthcheck]
                enabled = true

                [sinks.sink0.buffer]
                type = "memory"
                max_events = 500
                when_full = "block"
            "#}
            .to_string())
        );

        assert_eq!(
            generate_example(
                true,
                "/test_basic,test_basic,test_basic",
                &None,
                TransformInputsStrategy::Auto
            ),
            Ok(indoc::indoc! {r#"data_dir = "/var/lib/vector/"

                [transforms.transform0]
                inputs = []
                increase = 0.0
                suffix = ""
                type = "test_basic"

                [transforms.transform1]
                inputs = ["transform0"]
                increase = 0.0
                suffix = ""
                type = "test_basic"

                [transforms.transform2]
                inputs = ["transform1"]
                increase = 0.0
                suffix = ""
                type = "test_basic"
            "#}
            .to_string())
        );

        assert_eq!(
            generate_example(
                false,
                "/test_basic,test_basic,test_basic",
                &None,
                TransformInputsStrategy::Auto
            ),
            Ok(indoc::indoc! {r#"

                [transforms.transform0]
                inputs = []
                increase = 0.0
                suffix = ""
                type = "test_basic"

                [transforms.transform1]
                inputs = ["transform0"]
                increase = 0.0
                suffix = ""
                type = "test_basic"

                [transforms.transform2]
                inputs = ["transform1"]
                increase = 0.0
                suffix = ""
                type = "test_basic"
            "#}
            .to_string())
        );
    }
}
