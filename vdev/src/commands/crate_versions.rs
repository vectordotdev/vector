use std::{
    collections::{HashMap, HashSet},
    process::Command,
};

use anyhow::Result;
use clap::Args;
use itertools::Itertools as _;
use regex::Regex;

use crate::{app::CommandExt as _, utils};

/// Show information about crates versions pulled in by all dependencies
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// Show all versions, not just those that are duplicated
    #[arg(long)]
    all: bool,

    /// Features to activate (comma-separated, or set FEATURES env var)
    #[arg(short = 'F', long, value_delimiter = ',', env = "FEATURES")]
    features: Vec<String>,

    #[arg(long)]
    no_default_features: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let re_crate = Regex::new(r" (\S+) v([0-9.]+)").unwrap();
        let mut versions: HashMap<String, HashSet<String>> = HashMap::default();
        let features: Vec<String> = self
            .features
            .into_iter()
            .filter(|f| !f.is_empty())
            .collect();

        let mut cmd = Command::new("cargo");
        cmd.arg("tree");
        if self.no_default_features {
            cmd.arg("--no-default-features");
        }
        if !features.is_empty() {
            cmd.args(["--features", &features.join(",")]);
        }

        for line in cmd.check_output()?.lines() {
            if let Some(captures) = re_crate.captures(line) {
                let package = &captures[1];
                let version = &captures[2];
                versions
                    .entry(package.into())
                    .or_default()
                    .insert(version.into());
            }
        }

        if !self.all {
            versions.retain(|_, versions| versions.len() > 1);
        }

        let width = versions.keys().map(String::len).max().unwrap_or(0).max(7);
        if *utils::IS_A_TTY {
            println!("{:width$}  Version(s)", "Package");
            println!("{:width$}  ----------", "-------");
        }

        for (package, versions) in versions {
            let versions = versions.iter().join(" ");
            println!("{package:width$}  {versions}");
        }

        Ok(())
    }
}
