mod cmd;

use structopt::StructOpt;
use url::Url;

pub use cmd::cmd;

/// Encoding format for `event::LogEvent`s
#[derive(Debug, Clone, Copy)]
pub enum Encoding {
    Json,
    Yaml,
}

impl std::str::FromStr for Encoding {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "json" => Ok(Self::Json),
            "yaml" => Ok(Self::Yaml),
            _ => Err("Invalid encoding format".to_string()),
        }
    }
}

#[derive(StructOpt, Debug, Clone)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Interval to sample metrics at, in milliseconds
    #[structopt(default_value = "500", short = "i", long)]
    interval: u32,

    /// Vector GraphQL API server endpoint
    #[structopt(short, long)]
    url: Option<Url>,

    /// Sample log events to the provided limit
    #[structopt(default_value = "100", short = "l", long)]
    limit: u32,

    /// Encoding format for logs printed to screen
    #[structopt(default_value = "Encoding::Json", long)]
    encoding: Encoding,
}
