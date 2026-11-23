use anyhow::Result;

use crate::commands::changelog::FRAGMENT_TYPES;

/// Print the valid changelog fragment types and their descriptions, one per line.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        for fragment_type in FRAGMENT_TYPES {
            println!("{:<12} {}", fragment_type.name, fragment_type.description);
        }
        Ok(())
    }
}
