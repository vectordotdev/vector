mod cmd;

use clap::Parser;
pub(crate) use cmd::cmd;
use url::Url;
use vector_api_client::gql::TapEncodingFormat;

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
}
