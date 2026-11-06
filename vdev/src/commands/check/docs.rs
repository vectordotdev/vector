use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::{app, app::CommandExt, utils::paths::find_repo_root};

#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

const CUE_SCRIPT: &str = "scripts/cue.sh";
const CUE_DIR: &str = "website/cue";

/// Checks that the contents of the /website/cue folder are valid. This includes:
///
/// 1. Ensuring that the .cue files are properly formatted.
/// 2. Ensuring that the .cue files can compile.
impl Cli {
    pub fn exec(self) -> Result<()> {
        if Command::new("cue").arg("version").check_output().is_err() {
            bail!("Error: cue is not installed");
        }

        println!("Validating cue files formatting...");

        let unformatted_files = Self::get_unformatted_cue_files()?;

        if !unformatted_files.is_empty() {
            bail!(
                "Incorrectly formatted CUE files:\n\n{}\n\nRun './{} fmt' to fix formatting issues.",
                unformatted_files.join("\n"),
                CUE_SCRIPT
            );
        }

        println!("Validating cue files correctness...");

        if let Err(err) = app::exec(CUE_SCRIPT, ["vet"], true) {
            bail!("CUE files validation failed:\n{err}");
        }

        println!("Success! The contents of the sources in the './{CUE_DIR}' directory are valid");
        Ok(())
    }

    fn get_unformatted_cue_files() -> Result<Vec<String>> {
        let repo_root = find_repo_root()?;
        Self::find_unformatted_cue_files(&repo_root, || app::exec(CUE_SCRIPT, ["fmt"], true))
    }

    fn find_unformatted_cue_files<F>(repo_root: &Path, run_fmt: F) -> Result<Vec<String>>
    where
        F: FnOnce() -> Result<()>,
    {
        let source_dir = repo_root.join(CUE_DIR);
        let before = snapshot_cue_files(repo_root, &source_dir)?;

        run_fmt()?;

        let after = snapshot_cue_files(repo_root, &source_dir)?;

        let mut modified: Vec<String> = after
            .into_iter()
            .filter(|(path, content)| before.get(path) != Some(content))
            .map(|(path, _)| path)
            .collect();
        modified.sort();

        Ok(modified)
    }
}

fn snapshot_cue_files(repo_root: &Path, source_dir: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let files = collect_cue_files(source_dir)?;
    let mut snapshot = BTreeMap::new();

    for path in files {
        let content =
            fs::read(&path).with_context(|| format!("Failed to read {}", path.display()))?;
        let normalized = normalize_crlf(&content);
        let rel_path = relative_repo_path(repo_root, &path);
        snapshot.insert(rel_path, normalized);
    }

    Ok(snapshot)
}

fn collect_cue_files(source_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_cue_files(source_dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_cue_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_cue_files(&path, files)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("cue") {
            files.push(path);
        }
    }
    Ok(())
}

fn normalize_crlf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut iter = bytes.iter().peekable();
    while let Some(&b) = iter.next() {
        if b == b'\r' && iter.peek() == Some(&&b'\n') {
            continue;
        }
        out.push(b);
    }
    out
}

fn relative_repo_path(repo_root: &Path, path: &Path) -> String {
    let rel: &Path = path.strip_prefix(repo_root).unwrap_or(path);
    let mut normalized = PathBuf::new();
    for component in rel.components() {
        normalized.push(component);
    }
    normalized.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_preexisting_formatted_edits_and_reports_only_formatter_changes() {
        let temp = tempfile::tempdir().expect("create tempdir");
        let cue_dir = temp.path().join("website").join("cue");
        fs::create_dir_all(&cue_dir).expect("create cue dir");

        let clean_edited = cue_dir.join("already_formatted.cue");
        let unformatted = cue_dir.join("needs_fmt.cue");

        fs::write(&clean_edited, "package metadata\n\nfoo: \"bar\"\n").expect("write clean file");
        fs::write(&unformatted, "package metadata\nfoo:   \"baz\"\n")
            .expect("write unformatted file");

        let modified = Cli::find_unformatted_cue_files(temp.path(), || {
            // Simulate `cue fmt` leaving `already_formatted.cue` untouched while formatting `needs_fmt.cue`
            fs::write(&unformatted, "package metadata\n\nfoo: \"baz\"\n")?;
            Ok(())
        })
        .expect("check cue formatting");

        assert_eq!(modified, vec!["website/cue/needs_fmt.cue".to_string()]);
    }

    #[test]
    fn ignores_crlf_formatting_differences() {
        let temp = tempfile::tempdir().expect("create tempdir");
        let cue_dir = temp.path().join("website").join("cue");
        fs::create_dir_all(&cue_dir).expect("create cue dir");

        let file = cue_dir.join("test.cue");
        fs::write(&file, "package metadata\r\n\r\nfoo: \"bar\"\r\n").expect("write CRLF file");

        let modified = Cli::find_unformatted_cue_files(temp.path(), || {
            // Simulate `cue fmt` writing LF
            fs::write(&file, "package metadata\n\nfoo: \"bar\"\n")?;
            Ok(())
        })
        .expect("check cue formatting");

        assert!(modified.is_empty());
    }

    #[test]
    fn handles_path_with_brackets() {
        let temp = tempfile::Builder::new()
            .prefix("vector[1]")
            .tempdir()
            .expect("create tempdir");
        let cue_dir = temp.path().join("website").join("cue");
        fs::create_dir_all(&cue_dir).expect("create cue dir");

        let file = cue_dir.join("test.cue");
        fs::write(&file, "package metadata\n\nfoo: \"bar\"\n").expect("write file");

        let modified =
            Cli::find_unformatted_cue_files(temp.path(), || Ok(())).expect("check cue formatting");
        assert!(modified.is_empty());
    }
}
