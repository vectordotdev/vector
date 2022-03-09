mod cmd;
mod dashboard;
mod events;
mod metrics;
mod state;

use clap::Parser;
pub use cmd::cmd;
use url::Url;

#[derive(Parser, Debug, Clone)]
#[clap(rename_all = "kebab-case")]
pub struct Opts {
    /// Interval to sample metrics at, in milliseconds
    #[clap(default_value = "500", short = 'i', long)]
    interval: u32,

    /// Vector GraphQL API server endpoint
    #[clap(short, long)]
    url: Option<Url>,

    /// Humanize metrics, using numeric suffixes - e.g. 1,100 = 1.10 k, 1,000,000 = 1.00 M
    #[clap(short = 'H', long)]
    human_metrics: bool,

    /// Whether to reconnect if the underlying Vector API connection drops. By default, top will attempt to reconnect if the connection drops.
    #[clap(short, long)]
    no_reconnect: bool,
}
