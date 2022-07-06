use clap::Parser;
use serde::Serialize;

use crate::config::{SinkDescription, SourceDescription, TransformDescription};

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub struct Opts {
    /// Format the list in an encoding scheme.
    #[clap(long, default_value = "text", possible_values = &["text", "json", "avro"])]
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
    let sources = SourceDescription::types();
    let transforms = TransformDescription::types();
    let sinks = SinkDescription::types();

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
