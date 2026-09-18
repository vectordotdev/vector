use std::{fs, path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};
use clap::Args;
use glob::glob;

use crate::{
    app::CommandExt as _,
    utils::paths::{find_repo_root, resolve_repo_relative_path},
};

const CUE_SOURCES: &str = "website/cue";
const DEFAULT_OUTPUT: &str = "website/data/docs.json";

/// Build the CUE documentation model as the JSON consumed by Hugo and component-example tooling.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// Path for the rendered documentation JSON, relative to the repository root.
    #[arg(long, default_value = DEFAULT_OUTPUT)]
    output: PathBuf,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = find_repo_root()?;
        let output =
            resolve_repo_relative_path(&repo_root, &self.output, "Documentation JSON output")?;
        remove_docs(&output)?;
        write_docs(&output, &render_docs(&repo_root)?)
    }
}

/// Render the final CUE documentation model without relying on a file hand-off.
pub(crate) fn render_docs(repo_root: &std::path::Path) -> Result<Vec<u8>> {
    let source_dir = repo_root.join(CUE_SOURCES);
    let cue_files = cue_files(&source_dir)?;

    Command::new("cue")
        .arg("version")
        .current_dir(repo_root)
        .check_run()?;

    let output = Command::new("cue")
        .current_dir(repo_root)
        .arg("export")
        .arg("--all-errors")
        .args(cue_files)
        .output()
        .context("Failed to run `cue export`")?;
    if !output.status.success() {
        bail!(
            "`cue export` failed:\n{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(output.stdout)
}

pub(crate) fn write_docs(output: &std::path::Path, docs: &[u8]) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    remove_docs(output)?;
    fs::write(output, docs).with_context(|| format!("Failed to write {}", output.display()))
}

// CUE refuses to overwrite the prior docs model when it encounters errors, so remove it first
// rather than leaving stale data for the website or a subsequent generator to read.
fn remove_docs(output: &std::path::Path) -> Result<()> {
    match fs::remove_file(output) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to remove {}", output.display()));
        }
    }
    Ok(())
}

pub(super) fn cue_files(source_dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    let pattern = format!(
        "{}{}**{}*.cue",
        source_dir.display(),
        std::path::MAIN_SEPARATOR,
        std::path::MAIN_SEPARATOR
    );
    let mut files = glob(&pattern)
        .with_context(|| format!("Invalid CUE source glob: {pattern}"))?
        .collect::<Result<Vec<_>, _>>()?;
    files.retain(|path| path.is_file());
    files.sort();

    if files.is_empty() {
        bail!("No CUE sources found under {}", source_dir.display());
    }

    Ok(files)
}
