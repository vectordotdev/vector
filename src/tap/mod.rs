mod cmd;

use structopt::StructOpt;
use url::Url;

pub use cmd::cmd;

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
}
