use super::{load_builder_from_paths, load_source_from_paths, process_paths};

use crate::cli::handle_config_errors;
use crate::config;

use clap::Parser;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[clap(rename_all = "kebab-case")]
pub struct Opts {
    /// Pretty print JSON
    #[clap(short, long)]
    pretty: bool,

    /// Include default values where missing from config
    #[clap(short, long)]
    include_defaults: bool,

    /// Read configuration from one or more files. Wildcard paths are supported.
    /// File format is detected from the file name.
    /// If zero files are specified the default config path
    /// `/etc/vector/vector.toml` will be targeted.
    #[clap(
        name = "config",
        short,
        long,
        env = "VECTOR_CONFIG",
        use_value_delimiter(true)
    )]
    paths: Vec<PathBuf>,

    /// Vector config files in TOML format.
    #[clap(name = "config-toml", long, use_value_delimiter(true))]
    paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format.
    #[clap(name = "config-json", long, use_value_delimiter(true))]
    paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format.
    #[clap(name = "config-yaml", long, use_value_delimiter(true))]
    paths_yaml: Vec<PathBuf>,

    /// Read configuration from files in one or more directories.
    /// File format is detected from the file name.
    ///
    /// Files not ending in .toml, .json, .yaml, or .yml will be ignored.
    #[clap(
        name = "config-dir",
        short = 'C',
        long,
        env = "VECTOR_CONFIG_DIR",
        use_value_delimiter(true)
    )]
    pub config_dirs: Vec<PathBuf>,
}

impl Opts {
    fn paths_with_formats(&self) -> Vec<config::ConfigPath> {
        config::merge_path_lists(vec![
            (&self.paths, None),
            (&self.paths_toml, Some(config::Format::Toml)),
            (&self.paths_json, Some(config::Format::Json)),
            (&self.paths_yaml, Some(config::Format::Yaml)),
        ])
        .map(|(path, hint)| config::ConfigPath::File(path, hint))
        .chain(
            self.config_dirs
                .iter()
                .map(|dir| config::ConfigPath::Dir(dir.to_path_buf())),
        )
        .collect()
    }
}

/// Helper to merge JSON. Handles objects and array concatenation.
fn merge_json(a: &mut Value, b: Value) {
    match (a, b) {
        (Value::Object(ref mut a), Value::Object(b)) => {
            for (k, v) in b {
                merge_json(a.entry(k).or_insert(Value::Null), v);
            }
        }
        (a, b) => {
            *a = b;
        }
    }
}

/// Function used by the `vector config` subcommand for outputting a normalized configuration.
/// The purpose of this func is to combine user configuration after processing all paths,
/// Pipelines expansions, etc. The JSON result of this serialization can itself be used as a config,
/// which also makes it useful for version control or treating as a singular unit of configuration.
pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let paths = opts.paths_with_formats();
    // Start by serializing to a `ConfigBuilder`. This will leverage validation in config
    // builder fields which we'll use to error out if required.
    let (paths, builder) = match process_paths(&paths) {
        Some(paths) => match load_builder_from_paths(&paths) {
            Ok((builder, _)) => (paths, builder),
            Err(errs) => return handle_config_errors(errs),
        },
        None => return exitcode::CONFIG,
    };

    // Serialize source against normalized paths, and get a TOML `Table` as JSON.
    let mut source = match load_source_from_paths(&paths) {
        Ok((map, _)) => serde_json::to_value(map)
            .expect("should serialize config source to JSON. Please report."),
        Err(errs) => return handle_config_errors(errs),
    };

    // If a user has requested default fields, we'll serialize a `ConfigBuilder`. Otherwise,
    // we'll serialize the raw user provided config (without interpolated env vars, to preserve
    // the original source).
    if opts.include_defaults {
        // For security, we don't want environment variables to be interpolated in the final
        // output, but we *do* want defaults. To work around this, we'll serialize `ConfigBuilder`
        // to JSON, and merge in the raw config which will contain the pre-interpolated strings.
        let mut builder = serde_json::to_value(&builder)
            .expect("should serialize ConfigBuilder to JSON. Please report.");

        merge_json(&mut builder, source);

        source = builder
    }

    // Get a JSON string. This will either be pretty printed or (default) minified.
    let json = if opts.pretty {
        serde_json::to_string_pretty(&source)
    } else {
        serde_json::to_string(&source)
    };

    #[allow(clippy::print_stdout)]
    {
        println!("{}", json.expect("config should be serializable"));
    }

    exitcode::OK
}

#[cfg(test)]
mod tests {
    use super::merge_json;
    use serde_json::json;

    #[test]
    fn test_array_override() {
        let mut json = json!({
            "arr": [
                "value1", "value2"
            ]
        });

        let to_override = json!({
            "arr": [
                "value3", "value4"
            ]
        });

        merge_json(&mut json, to_override);

        assert_eq!(*json.get("arr").unwrap(), json!(["value3", "value4"]))
    }
}
