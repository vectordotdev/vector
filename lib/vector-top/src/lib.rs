//! Top subcommand
mod cmd;
mod dashboard;
mod events;
mod metrics;
mod state;

use std::net::{Ipv4Addr, SocketAddr};

use clap::Parser;
pub use cmd::{cmd, top};
pub use dashboard::is_tty;
use glob::Pattern;
use url::Url;

// FIXME duplicated code
/// By default, the API binds to 127.0.0.1:8686. This function should remain public;
/// `vector top`  will use it to determine which to connect to by default, if no URL
/// override is provided.
pub fn default_address() -> Option<SocketAddr> {
    Some(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8686))
}

/// Default GraphQL API address
pub fn default_graphql_url() -> Url {
    let addr = default_address().unwrap();
    Url::parse(&format!("http://{addr}/graphql"))
        .expect("Couldn't parse default API URL. Please report this.")
}

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

    /// Components IDs to observe (comma-separated; accepts glob patterns)
    #[arg(default_value = "*", value_delimiter(','), short = 'c', long)]
    components: Vec<Pattern>,
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

