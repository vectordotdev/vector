use anyhow::Result;
use clap::Args;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::path::PathBuf;

use crate::app::Application;

/// Custom Starship prompt plugin
#[derive(Args, Debug)]
#[command(hide = true)]
pub struct Cli {}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        let mut contexts = vec![];

        let path: PathBuf = [&app.repo.path, "Cargo.toml"].iter().collect();
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);

            for line in reader.lines() {
                if let Ok(line) = line {
                    if line.starts_with("version = ") {
                        contexts.push(format!("version: {}", &line[11..line.len() - 1]));
                        break;
                    }
                }
            }
        };

        contexts.push(format!("org: {}", app.config.org));
        app.display(format!("vector{{ {} }}", contexts.join(", ")));

        Ok(())
    }
}
