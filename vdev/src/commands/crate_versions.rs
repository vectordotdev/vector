use std::{collections::HashMap, collections::HashSet, process::Command};

use anyhow::Result;
use clap::Args;
use itertools::Itertools as _;
use regex::Regex;

use crate::{app::CommandExt as _, util};

/// Show information about crates versions pulled in by all dependencies
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// Show all versions, not just those that are duplicated
    #[arg(long)]
    all: bool,

    /// The feature to active (multiple allowed). If none are specified, the default is used.
    #[arg(short = 'F', long)]
    feature: Vec<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let re_crate = Regex::new(r" (\S+) v([0-9.]+)").unwrap();
        let mut versions: HashMap<String, HashSet<String>> = HashMap::default();

        for line in Command::new("cargo")
            .arg("tree")
            .features(&self.feature)
            .check_output()?
            .lines()
        {
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
        if *util::IS_A_TTY {
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
