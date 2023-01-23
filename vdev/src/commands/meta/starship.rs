use anyhow::Result;
use clap::Args;

use crate::util::CargoToml;

/// Custom Starship prompt plugin
#[derive(Args, Debug)]
#[command(hide = true)]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let mut contexts = vec![];

        if let Ok(cargo_toml) = CargoToml::load() {
            contexts.push(format!("version: {}", cargo_toml.package.version));
        }

        println!("vector{{ {} }}", contexts.join(", "));

        Ok(())
    }
}
