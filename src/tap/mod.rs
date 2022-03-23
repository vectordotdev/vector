mod cmd;

use clap::Parser;
pub(crate) use cmd::cmd;
use url::Url;
use vector_api_client::gql::TapEncodingFormat;

#[derive(Parser, Debug, Clone)]
#[clap(rename_all = "kebab-case")]
pub struct Opts {
    /// Interval to sample logs at, in milliseconds
    #[clap(default_value = "500", short = 'i', long)]
    interval: u32,

    /// Vector GraphQL API server endpoint
    #[clap(short, long)]
    url: Option<Url>,

    /// Maximum number of events to sample each interval
    #[clap(default_value = "100", short = 'l', long)]
    limit: u32,

    /// Encoding format for events printed to screen
    #[clap(default_value = "json", possible_values = &["json", "yaml", "logfmt"], short = 'f', long)]
    format: TapEncodingFormat,

    /// Components IDs to observe (comma-separated; accepts glob patterns)
    #[clap(use_value_delimiter(true))]
    component_id_patterns: Vec<String>,

    /// Components (sources, transforms) IDs whose outputs to observe (comma-separated; accepts glob patterns)
    #[clap(use_value_delimiter(true), long)]
    outputs_of: Vec<String>,

    /// Components (transforms, sinks) IDs whose inputs to observe (comma-separated; accepts glob patterns)
    #[clap(use_value_delimiter(true), long)]
    inputs_of: Vec<String>,

    /// Quiet output includes only events
    #[clap(short, long)]
    quiet: bool,

    /// Include metadata such as the event's associated component ID
    #[clap(short, long)]
    meta: bool,

    /// Whether to reconnect if the underlying Vector API connection drops. By default, tap will attempt to reconnect if the connection drops.
    #[clap(short, long)]
    no_reconnect: bool,
}
