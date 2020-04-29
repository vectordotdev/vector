use crate::topology::config::{SinkDescription, SourceDescription, TransformDescription};
use serde::Serialize;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Format the list in an encoding scheme.
    #[structopt(long, default_value = "text", possible_values = &["text", "json"])]
    format: Format,
}

#[derive(Debug, Clone, PartialEq)]
enum Format {
    Text,
    Json,
}

impl std::str::FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Format::Text),
            "json" => Ok(Format::Json),
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
    }

    exitcode::OK
}
