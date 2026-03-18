//! Top subcommand
mod cmd;

use clap::Parser;
use glob::Pattern;

pub use cmd::{cmd, top};
use url::Url;
use vector_lib::top::state::{FilterColumn, SortColumn};

use crate::config::api::default_grpc_url;

/// Top options
#[derive(Parser, Debug, Clone)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// Interval to sample metrics at, in milliseconds (must be >= 100)
    #[arg(default_value = "1000", short = 'i', long, value_parser = clap::value_parser!(u32).range(100..))]
    interval: u32,

    /// gRPC API server endpoint (e.g. http://localhost:8686)
    #[arg(short, long)]
    url: Option<Url>,

    /// Humanize metrics, using numeric suffixes - e.g. 1,100 = 1.10 k, 1,000,000 = 1.00 M
    #[arg(short = 'H', long, default_value_t = true)]
    human_metrics: bool,

    /// Whether to reconnect if the underlying API connection drops.
    ///
    /// By default, top will attempt to reconnect if the connection drops.
    #[arg(short, long)]
    no_reconnect: bool,

    /// Components IDs to observe (comma-separated; accepts glob patterns)
    #[arg(default_value = "*", value_delimiter(','), short = 'c', long)]
    components: Vec<Pattern>,

    /// Field to sort values to by default (can be changed while running).
    #[arg(short = 's', long)]
    sort_field: Option<SortColumn>,

    /// Sort descending instead of ascending.
    #[arg(long, default_value_t = false)]
    sort_desc: bool,

    /// Field to filter values by default (can be changed while running).
    #[arg(default_value = "id", long)]
    filter_field: FilterColumn,

    /// Filter to apply to the chosen field (ID by default).
    ///
    /// This accepts Regex patterns.
    #[arg(short = 'f', long)]
    filter_value: Option<String>,
}

impl Opts {
    /// Use the provided URL as the Vector gRPC API server, or default to the local port
    /// provided by the API config.
    pub fn url(&self) -> Url {
        self.url.clone().unwrap_or_else(default_grpc_url)
    }
}
