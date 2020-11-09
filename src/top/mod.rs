mod cmd;
mod dashboard;
mod events;
mod metrics;
mod state;

use structopt::StructOpt;
use url::Url;

pub use cmd::cmd;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// How often the screen refreshes (in milliseconds)
    #[structopt(default_value = "500", short = "i", long)]
    refresh_interval: u64,

    #[structopt(short, long)]
    url: Option<Url>,

    #[structopt(short, long)]
    human: bool,
}
