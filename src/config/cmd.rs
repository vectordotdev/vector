use super::{load_builder_from_paths, load_source_from_paths, process_paths, ConfigPath};
use crate::cli::handle_config_errors;
use structopt::StructOpt;

#[derive(StructOpt, Debug, Clone)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Pretty print JSON
    #[structopt(short, long)]
    pretty: bool,

    /// Include default values where missing from config
    #[structopt(short, long)]
    include_defaults: bool,
}

/// Function used by the `vector config` subcommand for outputting a normalized configuration.
/// The purpose of this func is to combine user configuration after processing all paths,
/// Pipelines expansions, etc. The JSON result of this serialization can itself be used as a config,
/// which also makes it useful for version control or treating as a singular unit of configuration.
pub fn cmd(opts: &Opts, config_paths: &[ConfigPath]) -> exitcode::ExitCode {
    // Start by serializing to a `ConfigBuilder`. This will leverage validation in config
    // builder fields which we'll use to error out if required.
    let (paths, builder) = match process_paths(config_paths) {
        Some(paths) => match load_builder_from_paths(&paths) {
            Ok((builder, _)) => (paths, builder),
            Err(errs) => return handle_config_errors(errs),
        },
        None => return exitcode::CONFIG,
    };

    // If a user has requested default fields, we'll serialize a `ConfigBuilder`. Otherwise,
    // we'll serialize the raw user provided config (without interpolated env vars, to preserve
    // the original source).
    let json = if opts.include_defaults {
        if opts.pretty {
            serde_json::to_string_pretty(&builder)
        } else {
            serde_json::to_string(&builder)
        }
    } else {
        // Serialize source against normalized paths, and get a TOML `Table`.
        let map = match load_source_from_paths(&paths) {
            Ok((map, _)) => map,
            Err(errs) => return handle_config_errors(errs),
        };

        if opts.pretty {
            serde_json::to_string_pretty(&map)
        } else {
            serde_json::to_string(&map)
        }
    };

    println!("{}", json.expect("config should be serializable"));

    exitcode::OK
}
