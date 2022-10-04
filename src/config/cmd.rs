use std::path::PathBuf;

use clap::Parser;

use super::{load_builder_from_paths, load_source_from_paths, process_paths};
use crate::cli::handle_config_errors;
use crate::config;

#[derive(Parser, Debug, Clone)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// Pretty print JSON
    #[arg(short, long)]
    pretty: bool,

    /// Include default values where missing from config
    #[arg(short, long)]
    include_defaults: bool,

    /// Read configuration from one or more files. Wildcard paths are supported.
    /// File format is detected from the file name.
    /// If zero files are specified the default config path
    /// `/etc/vector/vector.toml` will be targeted.
    #[arg(
        id = "config",
        short,
        long,
        env = "VECTOR_CONFIG",
        value_delimiter(',')
    )]
    paths: Vec<PathBuf>,

    /// Vector config files in TOML format.
    #[arg(id = "config-toml", long, value_delimiter(','))]
    paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format.
    #[arg(id = "config-json", long, value_delimiter(','))]
    paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format.
    #[arg(id = "config-yaml", long, value_delimiter(','))]
    paths_yaml: Vec<PathBuf>,

    /// Read configuration from files in one or more directories.
    /// File format is detected from the file name.
    ///
    /// Files not ending in .toml, .json, .yaml, or .yml will be ignored.
    #[arg(
        id = "config-dir",
        short = 'C',
        long,
        env = "VECTOR_CONFIG_DIR",
        value_delimiter(',')
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

    // Load source TOML.
    let source = match load_source_from_paths(&paths) {
        Ok((map, _)) => map,
        Err(errs) => return handle_config_errors(errs),
    };

    let json = super::util::json::serialize(source, &builder, opts.include_defaults, opts.pretty);
    let json = json.expect("config should be serializable");

    #[cfg(feature = "enterprise")]
    if let Err(errs) = super::loading::schema::check_sensitive_fields_from_string(&json, &builder) {
        return handle_config_errors(errs);
    }

    #[allow(clippy::print_stdout)]
    {
        println!("{}", json);
    }

    exitcode::OK
}

#[cfg(all(test, feature = "sources", feature = "transforms", feature = "sinks"))]
mod tests {
    use std::collections::HashMap;

    use proptest::{num, prelude::*, sample};
    use rand::{
        prelude::{SliceRandom, StdRng},
        SeedableRng,
    };
    use serde_json::json;
    use vector_config::component::{SinkDescription, SourceDescription, TransformDescription};

    use crate::{
        config::util::json::{merge as merge_json, serialize as serialize_to_json},
        config::{vars, ConfigBuilder},
        generate::{generate_example, TransformInputsStrategy},
    };

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

    #[test]
    fn include_defaults_does_not_include_env_vars() {
        let env_var = "VECTOR_CONFIG_INCLUDE_DEFAULTS_TEST";
        let env_var_in_arr = "VECTOR_CONFIG_INCLUDE_DEFAULTS_TEST_IN_ARR";

        let config_source = format!(
            r#"
            [sources.in]
            type = "demo_logs"
            format = "${{{}}}"

            [sinks.out]
            type = "blackhole"
            inputs = ["${{{}}}"]
        "#,
            env_var, env_var_in_arr
        );
        let (interpolated_config_source, _) = vars::interpolate(
            config_source.as_ref(),
            &HashMap::from([
                (env_var.to_string(), "syslog".to_string()),
                (env_var_in_arr.to_string(), "in".to_string()),
            ]),
        )
        .unwrap();

        let json: serde_json::Value = serde_json::from_str(
            serialize_to_json(
                toml::from_str(config_source.as_ref()).unwrap(),
                &ConfigBuilder::from_toml(interpolated_config_source.as_ref()),
                true,
                false,
            )
            .unwrap()
            .as_ref(),
        )
        .unwrap();

        assert_eq!(
            json["sources"]["in"]["format"],
            json!(format!("${{{}}}", env_var))
        );
        assert_eq!(
            json["sinks"]["out"]["inputs"],
            json!(vec![format!("${{{}}}", env_var_in_arr)])
        );
    }

    /// Select any 2-4 sources
    fn arb_sources() -> impl Strategy<Value = Vec<&'static str>> {
        sample::subsequence(SourceDescription::types(), 2..=4)
    }

    /// Select any 2-4 transforms
    fn arb_transforms() -> impl Strategy<Value = Vec<&'static str>> {
        sample::subsequence(TransformDescription::types(), 2..=4)
    }

    /// Select any 2-4 sinks
    fn arb_sinks() -> impl Strategy<Value = Vec<&'static str>> {
        sample::subsequence(SinkDescription::types(), 2..=4)
    }

    fn create_config_source(sources: &[&str], transforms: &[&str], sinks: &[&str]) -> String {
        // This creates a string in the syntax expected by the `vector generate`
        // command whose internal mechanics we are using to create valid Vector
        // configurations.
        //
        // Importantly, we have to name the components (in this case, simply by
        // their type as each type of component is guaranteed to only appear
        // once), because (in some tests) we'd like to shuffle the configuration
        // later in a way that does not change its actual semantics. Otherwise,
        // an autogenerated ID like `source0` could correspond to different
        // sources depending on the ordering of the `vector generate` input.
        //
        // We also append a fixed `remap` transform to the transforms list. This
        // ensures sink inputs are consistent since `generate` uses the last
        // transform the input for each sink.
        let generate_config_str = format!(
            "{}/{}/{}",
            sources
                .iter()
                .map(|source| format!("{}:{}", source, source))
                .collect::<Vec<_>>()
                .join(","),
            transforms
                .iter()
                .map(|transform| format!("{}:{}", transform, transform))
                .chain(vec!["manually-added-remap:remap".to_string()])
                .collect::<Vec<_>>()
                .join(","),
            sinks
                .iter()
                .map(|sink| format!("{}:{}", sink, sink))
                .collect::<Vec<_>>()
                .join(","),
        );
        generate_example(
            false,
            generate_config_str.as_ref(),
            &None,
            TransformInputsStrategy::All,
        )
        .expect("invalid config generated")
    }

    proptest! {
        #[test]
        /// Output should be the same regardless of input config ordering
        fn output_has_consistent_ordering(mut sources in arb_sources(), mut transforms in arb_transforms(), mut sinks in arb_sinks(), seed in num::u64::ANY) {
            let config_source = create_config_source(sources.as_ref(), transforms.as_ref(), sinks.as_ref());

            // Shuffle the ordering of components which shuffles the order in
            // which items appear in the TOML config
            let mut rng = StdRng::seed_from_u64(seed);
            sources.shuffle(&mut rng);
            transforms.shuffle(&mut rng);
            sinks.shuffle(&mut rng);
            let shuffled_config_source = create_config_source(sources.as_ref(), transforms.as_ref(), sinks.as_ref());

            let json = serialize_to_json(
                toml::from_str(config_source.as_ref()).unwrap(),
                &ConfigBuilder::from_toml(config_source.as_ref()),
                false,
                false
            )
            .unwrap();
            let shuffled_json = serialize_to_json(
                toml::from_str(shuffled_config_source.as_ref()).unwrap(),
                &ConfigBuilder::from_toml(shuffled_config_source.as_ref()),
                false,
                false
            )
            .unwrap();

            assert_eq!(json, shuffled_json);
        }
    }

    proptest! {
        #[test]
        /// Output is a valid configuration
        fn output_is_a_valid_config(sources in arb_sources(), transforms in arb_transforms(), sinks in arb_sinks()) {
            let config_source = create_config_source(sources.as_ref(), transforms.as_ref(), sinks.as_ref());
            let json = serialize_to_json(
                toml::from_str(config_source.as_ref()).unwrap(),
                &ConfigBuilder::from_toml(config_source.as_ref()),
                false,
                false
            )
            .unwrap();
            assert!(serde_json::from_str::<ConfigBuilder>(json.as_ref()).is_ok());
        }
    }
}
