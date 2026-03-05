use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::utils::git::sparse_checkout_docs;
use crate::utils::paths::find_repo_root;

const VRL_REPO_URL: &str = "https://github.com/vectordotdev/vrl.git";
const VECTOR_REPO_URL: &str = "https://github.com/vectordotdev/vector.git";
const VRL_PACKAGE_NAME: &str = "vrl";

/// Generate VRL function documentation by fetching pre-built JSON docs from the VRL and Vector
/// repositories.
///
/// VRL stdlib docs come from the VRL repo (`docs/generated/*.json`), and Vector-specific function
/// docs come from the Vector repo (`docs/generated/*.json`). Both sets are merged into a single
/// `generated.cue` output file.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Output directory for the generated.cue file
    #[arg(short, long)]
    output_dir: PathBuf,

    /// VRL commit SHA to fetch docs from. If unspecified, read from Cargo.lock.
    #[arg(long)]
    vrl_sha: Option<String>,

    /// Vector commit SHA to fetch docs from. If unspecified, read docs/generated locally.
    #[arg(long)]
    vector_sha: Option<String>,
}

#[derive(Serialize)]
struct FunctionDocWrapper {
    remap: RemapWrapper,
}

#[derive(Serialize)]
struct RemapWrapper {
    functions: BTreeMap<String, Value>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = find_repo_root()?;
        let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;

        // VRL stdlib docs
        let vrl_sha = match self.vrl_sha {
            Some(sha) => sha,
            None => get_vrl_commit_sha(&repo_root)?,
        };
        info!("VRL commit SHA: {vrl_sha}");

        let vrl_clone_dir = temp_dir.path().join("vrl");
        sparse_checkout_docs(&vrl_sha, VRL_REPO_URL, &vrl_clone_dir)?;
        let vrl_docs_dir = vrl_clone_dir.join("docs").join("generated");

        let mut functions = read_function_docs(&vrl_docs_dir)?;
        info!("Read {} VRL stdlib function docs", functions.len());

        // Vector-specific docs
        let vector_docs_dir = if let Some(vector_sha) = &self.vector_sha {
            info!("Vector commit SHA: {vector_sha}");
            let vector_clone_dir = temp_dir.path().join("vector");
            sparse_checkout_docs(vector_sha, VECTOR_REPO_URL, &vector_clone_dir)?;
            vector_clone_dir.join("docs").join("generated")
        } else {
            repo_root.join("docs").join("generated")
        };

        let vector_functions = read_function_docs(&vector_docs_dir)?;
        info!("Read {} Vector function docs", vector_functions.len());
        functions.extend(vector_functions);

        let wrapper = FunctionDocWrapper {
            remap: RemapWrapper { functions },
        };

        fs::create_dir_all(&self.output_dir)?;
        let mut json = serde_json::to_string(&wrapper)?;
        json.push('\n');
        let filepath = self.output_dir.join("generated.cue");
        fs::write(&filepath, json)?;

        info!("Generated: {}", filepath.display());
        Ok(())
    }
}

/// A minimal representation of a `[[package]]` entry in `Cargo.lock`.
#[derive(Deserialize)]
struct LockPackage {
    name: String,
    source: Option<String>,
}

#[derive(Deserialize)]
struct CargoLock {
    package: Vec<LockPackage>,
}

/// Parse `Cargo.lock` to find the git commit SHA for the `vrl` package.
fn get_vrl_commit_sha(repo_root: &Path) -> Result<String> {
    let lock_path = repo_root.join("Cargo.lock");
    let lock_text = fs::read_to_string(&lock_path)
        .with_context(|| format!("Failed to read {}", lock_path.display()))?;

    let lock: CargoLock =
        toml::from_str(&lock_text).context("Failed to parse Cargo.lock as TOML")?;

    let pkg = lock
        .package
        .iter()
        .find(|p| {
            p.name == VRL_PACKAGE_NAME
                && p.source
                    .as_deref()
                    .is_some_and(|s| s.contains("github.com/vectordotdev/vrl"))
        })
        .context("Could not find VRL package with git source in Cargo.lock")?;

    // Source format: "git+https://github.com/vectordotdev/vrl.git?branch=doc-generation#5316c01b..."
    let source = pkg.source.as_deref().unwrap();
    let sha = source
        .rsplit_once('#')
        .map(|(_, sha)| sha)
        .context("Could not extract commit SHA from VRL source string")?;

    Ok(sha.to_string())
}

/// Read all `*.json` files from a directory into a name->value map.
fn read_function_docs(docs_dir: &Path) -> Result<BTreeMap<String, Value>> {
    let mut functions = BTreeMap::new();

    let entries: Vec<_> = fs::read_dir(docs_dir)
        .with_context(|| format!("Failed to read docs directory: {}", docs_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to iterate docs directory")?;

    for entry in entries {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let value: Value = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse JSON from {}", path.display()))?;

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("Invalid filename")?
                .to_string();

            functions.insert(name, value);
        }
    }

    Ok(functions)
}
