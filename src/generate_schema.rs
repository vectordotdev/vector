//! Vector `generate-schema` command implementation.

use clap::Parser;
use std::fs;
use std::path::PathBuf;
use vector_lib::configurable::schema::generate_root_schema;

use crate::config::ConfigBuilder;

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
/// Command line options for the `generate-schema` command.
pub struct Opts {
    /// File path to
    #[arg(short, long)]
    pub(crate) output_path: Option<PathBuf>,
}

/// Execute the `generate-schema` command.
#[allow(clippy::print_stdout, clippy::print_stderr)]
pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    match generate_root_schema::<ConfigBuilder>() {
        Ok(schema) => {
            let json = serde_json::to_string_pretty(&schema)
                .expect("rendering root schema to JSON should not fail");

            if let Some(output_path) = &opts.output_path {
                if output_path.exists() {
                    eprintln!("Error: Output file {output_path:?} already exists");
                    return exitcode::CANTCREAT;
                }

                return match fs::write(output_path, json) {
                    Ok(_) => {
                        println!("Schema successfully written to {output_path:?}");
                        exitcode::OK
                    }
                    Err(e) => {
                        eprintln!("Error writing to file {output_path:?}: {e:?}");
                        exitcode::IOERR
                    }
                };
            } else {
                println!("{json}");
            }
            exitcode::OK
        }
        Err(e) => {
            eprintln!("error while generating schema: {:?}", e);
            exitcode::SOFTWARE
        }
    }
}
