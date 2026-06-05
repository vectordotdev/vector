//! Vector `generate-schema` command implementation.

use std::{fs, path::PathBuf};

use clap::Parser;
use serde_json::{Value, json};
use vector_common::internal_event::{CounterName, GaugeName, HistogramName};
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

fn metric_enum_schema<T: vector_lib::configurable::Configurable + 'static>() -> Value {
    generate_root_schema::<T>()
        .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
        .unwrap_or(Value::Null)
}

/// Execute the `generate-schema` command.
#[allow(clippy::print_stdout, clippy::print_stderr)]
pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    match generate_root_schema::<ConfigBuilder>() {
        Ok(config_schema) => {
            // Convert to Value so we can inject the metric enum schemas.
            let mut schema = serde_json::to_value(config_schema)
                .expect("rendering root schema to JSON should not fail");

            // Inject metric name enum schemas so vdev can generate
            // internal_metrics.cue output descriptions from them.
            schema["_metric_schemas"] = json!({
                "counters":   metric_enum_schema::<CounterName>(),
                "histograms": metric_enum_schema::<HistogramName>(),
                "gauges":     metric_enum_schema::<GaugeName>(),
            });

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
            eprintln!("error while generating schema: {e:?}");
            exitcode::SOFTWARE
        }
    }
}
