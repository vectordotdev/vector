//! Top subcommand
mod cmd;
mod dashboard;
mod events;
mod metrics;
mod state;

use clap::Parser;
pub use cmd::cmd;
pub use cmd::top;
pub use dashboard::is_tty;
use url::Url;

use crate::config::api::default_graphql_url;

/// Top options
#[derive(Parser, Debug, Clone)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// Interval to sample metrics at, in milliseconds
    #[arg(default_value = "1000", short = 'i', long)]
    interval: u32,

    /// GraphQL API server endpoint
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
}

impl Opts {
    /// Use the provided URL as the Vector GraphQL API server, or default to the local port
    /// provided by the API config.
    pub fn url(&self) -> Url {
        self.url.clone().unwrap_or_else(default_graphql_url)
    }

    /// URL with scheme set to WebSockets
    pub fn web_socket_url(&self) -> Url {
        let mut url = self.url();
        url.set_scheme(match url.scheme() {
            "https" => "wss",
            _ => "ws",
        })
        .expect("Couldn't build WebSocket URL. Please report.");

        url
    }
}
