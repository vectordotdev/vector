#![allow(missing_docs)]
use clap::Parser;
use serde::Serialize;

use vector_lib::configurable::component::{
    EnrichmentTableDescription, SinkDescription, SourceDescription, TransformDescription,
};

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// Format the list in an encoding scheme.
    #[arg(long, default_value = "text")]
    format: Format,
}

#[derive(clap::ValueEnum, Debug, Clone, PartialEq)]
enum Format {
    Text,
    Json,
    Avro,
}

#[derive(Serialize)]
pub struct EncodedList {
    sources: Vec<&'static str>,
    transforms: Vec<&'static str>,
    sinks: Vec<&'static str>,
    enrichment_tables: Vec<&'static str>,
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let sources = SourceDescription::types();
    let transforms = TransformDescription::types();
    let sinks = SinkDescription::types();
    let enrichment_tables = EnrichmentTableDescription::types();

    #[allow(clippy::print_stdout)]
    match opts.format {
        Format::Text => {
            println!("Sources:");
            for name in sources {
                println!("- {}", name);
            }

            println!("\nTransforms:");
            for name in transforms {
                println!("- {}", name);
            }

            println!("\nSinks:");
            for name in sinks {
                println!("- {}", name);
            }

            println!("\nEnrichment tables:");
            for name in enrichment_tables {
                println!("- {}", name);
            }
        }
        Format::Json => {
            let list = EncodedList {
                sources,
                transforms,
                sinks,
                enrichment_tables,
            };
            println!("{}", serde_json::to_string(&list).unwrap());
        }
        Format::Avro => {
            let list = EncodedList {
                sources,
                transforms,
                sinks,
                enrichment_tables,
            };
            println!("{}", serde_json::to_string(&list).unwrap());
        }
    }

    exitcode::OK
}
