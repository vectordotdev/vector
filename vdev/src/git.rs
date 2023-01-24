use std::{collections::HashSet, process::Command};

use anyhow::Result;

use crate::app::CommandExt as _;

pub fn current_branch() -> Result<String> {
    let output = capture_output(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    Ok(output.trim_end().to_string())
}

pub fn changed_files() -> Result<Vec<String>> {
    let mut files = HashSet::new();

    // Committed e.g.:
    // A   relative/path/to/file.added
    // M   relative/path/to/file.modified
    let output = capture_output(&["diff", "--name-status", "origin/master..."])?;
    for line in output.lines() {
        if !is_warning_line(line) {
            if let Some((_, path)) = line.split_once('\t') {
                files.insert(path.to_string());
            }
        }
    }

    // Tracked
    let output = capture_output(&["diff", "--name-only", "HEAD"])?;
    for line in output.lines() {
        if !is_warning_line(line) {
            files.insert(line.to_string());
        }
    }

    // Untracked
    let output = capture_output(&["ls-files", "--others", "--exclude-standard"])?;
    for line in output.lines() {
        files.insert(line.to_string());
    }

    let mut sorted = Vec::from_iter(files);
    sorted.sort();

    Ok(sorted)
}

pub fn list_files() -> Result<Vec<String>> {
    Ok(capture_output(&["ls-files"])?
        .lines()
        .map(str::to_owned)
        .collect())
}

fn capture_output(args: &[&str]) -> Result<String> {
    Command::new("git").in_repo().args(args).capture_output()
}

fn is_warning_line(line: &str) -> bool {
    line.starts_with("warning: ") || line.contains("original line endings")
}
