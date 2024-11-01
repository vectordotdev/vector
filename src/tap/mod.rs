//! Tap subcommand
mod cmd;

use clap::Parser;
pub(crate) use cmd::cmd;
pub use cmd::tap;
use url::Url;
use vector_lib::api_client::gql::TapEncodingFormat;

use crate::config::api::default_graphql_url;

/// Tap options
#[derive(Parser, Debug, Clone)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// Interval to sample logs at, in milliseconds
    #[arg(default_value = "500", short = 'i', long)]
    interval: u32,

    /// GraphQL API server endpoint
    #[arg(short, long)]
    url: Option<Url>,

    /// Maximum number of events to sample each interval
    #[arg(default_value = "100", short = 'l', long)]
    limit: u32,

    /// Encoding format for events printed to screen
    #[arg(default_value = "json", short = 'f', long)]
    format: TapEncodingFormat,

    /// Components IDs to observe (comma-separated; accepts glob patterns)
    #[arg(value_delimiter(','))]
    component_id_patterns: Vec<String>,

    /// Components (sources, transforms) IDs whose outputs to observe (comma-separated; accepts glob patterns)
    #[arg(value_delimiter(','), long)]
    outputs_of: Vec<String>,

    /// Components (transforms, sinks) IDs whose inputs to observe (comma-separated; accepts glob patterns)
    #[arg(value_delimiter(','), long)]
    inputs_of: Vec<String>,

    /// Quiet output includes only events
    #[arg(short, long)]
    quiet: bool,

    /// Include metadata such as the event's associated component ID
    #[arg(short, long)]
    meta: bool,

    /// Whether to reconnect if the underlying API connection drops. By default, tap will attempt to reconnect if the connection drops.
    #[arg(short, long)]
    no_reconnect: bool,

    /// Specifies a duration (in milliseconds) to sample logs (e.g. specifying 10000 will sample logs for 10 seconds then exit)
    #[arg(short = 'd', long)]
    duration_ms: Option<u64>,
}

impl Opts {
    /// Component ID patterns to tap
    ///
    /// If no patterns are provided, tap all components' outputs
    pub fn outputs_patterns(&self) -> Vec<String> {
        if self.component_id_patterns.is_empty()
            && self.outputs_of.is_empty()
            && self.inputs_of.is_empty()
        {
            vec!["*".to_string()]
        } else {
            self.outputs_of
                .iter()
                .cloned()
                .chain(self.component_id_patterns.iter().cloned())
                .collect()
        }
    }

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
