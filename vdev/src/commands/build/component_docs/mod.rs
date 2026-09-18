use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

mod runner;
mod schema;

/// Build the component documentation by parsing the JSON configuration schema and generating cue files.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The path to the configuration schema JSON file
    configuration_schema: PathBuf,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        runner::run(&self.configuration_schema)
    }
}
