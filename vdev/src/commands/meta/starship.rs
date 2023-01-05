use anyhow::Result;
use clap::Args;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::path::PathBuf;

use crate::app;

const VERSION_START: &str = "version = ";

/// Custom Starship prompt plugin
#[derive(Args, Debug)]
#[command(hide = true)]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let mut contexts = vec![];

        let path: PathBuf = [app::path(), "Cargo.toml"].iter().collect();
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                if line.starts_with(VERSION_START) {
                    contexts.push(format!(
                        "version: {}",
                        &line[VERSION_START.len() + 1..line.len() - 1]
                    ));
                    break;
                }
            }
        };

        display!("vector{{ {} }}", contexts.join(", "));

        Ok(())
    }
}
