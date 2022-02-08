use std::collections::HashSet;

use serde::Serialize;
use structopt::StructOpt;

use crate::config::{SinkDescription, SourceDescription, TransformDescription};

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Format the list in an encoding scheme.
    #[structopt(long, default_value = "text", possible_values = &["text", "json", "avro"])]
    format: Format,
}

#[derive(Debug, Clone, PartialEq)]
enum Format {
    Text,
    Json,
    Avro,
}

impl std::str::FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Format::Text),
            "json" => Ok(Format::Json),
            "avro" => Ok(Format::Avro),
            s => Err(format!(
                "{} is not a valid option, expected `text` or `json`",
                s
            )),
        }
    }
}

#[derive(Serialize)]
pub struct EncodedList {
    sources: Vec<&'static str>,
    transforms: Vec<&'static str>,
    sinks: Vec<&'static str>,
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let mut sources = SourceDescription::types();
    let mut transforms = TransformDescription::types();
    let mut sinks = SinkDescription::types();

    // Remove deprecated components from list
    let deprecated = deprecated_components();
    sources.retain(|name| !deprecated.contains(name));
    transforms.retain(|name| !deprecated.contains(name));
    sinks.retain(|name| !deprecated.contains(name));

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
        }
        Format::Json => {
            let list = EncodedList {
                sources,
                transforms,
                sinks,
            };
            println!("{}", serde_json::to_string(&list).unwrap());
        }
        Format::Avro => {
            let list = EncodedList {
                sources,
                transforms,
                sinks,
            };
            println!("{}", serde_json::to_string(&list).unwrap());
        }
    }

    exitcode::OK
}

/// Returns names of all deprecated components.
fn deprecated_components() -> HashSet<&'static str> {
    vec!["field_filter"].into_iter().collect()
}
