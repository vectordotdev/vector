use super::{load_builder_from_paths, process_paths, ConfigPath};
use crate::cli::handle_config_errors;
use structopt::StructOpt;

#[derive(StructOpt, Debug, Clone)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Pretty print JSON
    #[structopt(short, long)]
    pretty: bool,
}

/// Function used by the `vector config` subcommand for outputting a normalized configuration.
/// The purpose of this func is to combine user configuration after processing all paths,
/// Pipelines expansions, etc. The JSON result of this serialization can itself be used as a config,
/// which also makes it useful for version control or treating as a singular unit of configuration.
pub fn cmd(opts: &Opts, config_paths: &[ConfigPath]) -> exitcode::ExitCode {
    let builder = match process_paths(&config_paths) {
        Some(paths) => match load_builder_from_paths(&paths) {
            Ok((builder, _)) => builder,
            Err(errs) => return handle_config_errors(errs),
        },
        None => return exitcode::CONFIG,
    };

    let json = if opts.pretty {
        serde_json::to_string_pretty(&builder)
    } else {
        serde_json::to_string(&builder)
    };

    println!("{}", json.expect("config should be serializable"));

    exitcode::OK
}
