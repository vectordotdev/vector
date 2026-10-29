use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

use crate::app;

/// Check for advisories, licenses, and sources for crate dependencies
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Only check licenses
    #[arg(long)]
    licenses_only: bool,

    /// Checkout of the base revision. Only fail on errors that this checkout does not also have.
    #[arg(long, value_name = "DIR")]
    base_dir: Option<PathBuf>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let Some(base_dir) = &self.base_dir else {
            return app::exec("cargo", self.args("human"), true);
        };

        let head = self.errors(Path::new(app::path()))?;
        if head.is_empty() {
            println!("No errors found.");
            return Ok(());
        }

        // The base runs with the `cargo deny` arguments of this branch. It fetches the advisory
        // database after the run above, so an advisory published in between can not show up on
        // this branch only.
        let base: BTreeSet<String> = self
            .errors(base_dir)?
            .into_iter()
            .map(|error| error.key)
            .collect();

        let (existing, new): (Vec<_>, Vec<_>) =
            head.iter().partition(|error| base.contains(&error.key));
        if !existing.is_empty() {
            println!("Errors that the base also has (ignored):");
            for error in &existing {
                println!("{error}");
            }
        }
        if new.is_empty() {
            println!("No new errors compared to the base.");
            return Ok(());
        }
        println!("New errors compared to the base:");
        for error in &new {
            println!("{error}");
        }
        bail!("This change adds cargo deny errors. Run `make check-deny` for details.")
    }

    fn args<'a>(&self, format: &'a str) -> [&'a str; 8] {
        let check = if self.licenses_only {
            "licenses"
        } else {
            "all"
        };
        [
            "deny",
            "--log-level",
            "error",
            "--format",
            format,
            "--all-features",
            "check",
            check,
        ]
    }

    /// Run `cargo deny` in `dir` and return its errors.
    fn errors(&self, dir: &Path) -> Result<BTreeSet<DenyError>> {
        let output = Command::new("cargo")
            .args(self.args("json"))
            .current_dir(dir)
            .output()?;
        // Errors make `cargo deny` fail, so look for the summary to see if it completed.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut completed = false;
        let mut errors = BTreeSet::new();
        for line in stderr
            .lines()
            .filter_map(|line| serde_json::from_str::<Line>(line).ok())
        {
            match line.kind.as_str() {
                "summary" => completed = true,
                "diagnostic" => {
                    let diagnostic = Diagnostic::deserialize(&line.fields).with_context(|| {
                        format!("unexpected cargo deny diagnostic: {}", line.fields)
                    })?;
                    if diagnostic.severity == "error" {
                        errors.insert(DenyError::from(diagnostic));
                    }
                }
                _ => {}
            }
        }
        if !completed {
            bail!(
                "cargo deny did not complete in {}:\n{}{stderr}",
                dir.display(),
                String::from_utf8_lossy(&output.stdout),
            );
        }
        Ok(errors)
    }
}

/// One line of `cargo deny --format json` output.
#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    fields: serde_json::Value,
}

#[derive(Deserialize)]
struct Diagnostic {
    severity: String,
    code: String,
    message: String,
    advisory: Option<Advisory>,
    #[serde(default)]
    graphs: Vec<Graph>,
}

#[derive(Deserialize)]
struct Advisory {
    id: String,
}

#[derive(Deserialize)]
struct Graph {
    #[serde(rename = "Krate")]
    krate: Krate,
}

#[derive(Deserialize)]
struct Krate {
    name: String,
    version: String,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DenyError {
    /// Identifies the error across revisions. Uses the advisory ID, so that a version change of
    /// an affected crate does not make the error new.
    key: String,
    description: String,
}

impl From<Diagnostic> for DenyError {
    fn from(diagnostic: Diagnostic) -> Self {
        let crates = diagnostic
            .graphs
            .iter()
            .map(|graph| format!("{} {}", graph.krate.name, graph.krate.version))
            .collect::<Vec<_>>()
            .join(", ");
        let key = match diagnostic.advisory {
            Some(advisory) => advisory.id,
            None => match diagnostic.graphs.first() {
                Some(graph) => format!("{} {}", diagnostic.code, graph.krate.name),
                None => diagnostic.code,
            },
        };
        Self {
            key,
            description: format!("{crates}: {}", diagnostic.message),
        }
    }
}

impl fmt::Display for DenyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\t{}", self.key, self.description)
    }
}
