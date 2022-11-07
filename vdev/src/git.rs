use anyhow::Result;
use std::collections::HashSet;
use std::process::Command;

use crate::app;

pub fn current_branch() -> Result<String> {
    let output = get_output(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    Ok(output.trim_end().to_string())
}

pub fn changed_files() -> Result<Vec<String>> {
    let mut files = HashSet::new();

    // Committed e.g.:
    // A   relative/path/to/file.added
    // M   relative/path/to/file.modified
    let output = get_output(&["diff", "--name-status", "origin/master..."])?;
    for line in output.lines() {
        if !is_warning_line(line) {
            if let Some((_, path)) = line.split_once("\t") {
                files.insert(path.to_string());
            }
        }
    }

    // Tracked
    let output = get_output(&["diff", "--name-only", "HEAD"])?;
    for line in output.lines() {
        if !is_warning_line(line) {
            files.insert(line.to_string());
        }
    }

    // Untracked
    let output = get_output(&["ls-files", "--others", "--exclude-standard"])?;
    for line in output.lines() {
        files.insert(line.to_string());
    }

    let mut sorted = Vec::from_iter(files);
    sorted.sort();

    Ok(sorted)
}

fn construct_command(args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(args);
    cmd.current_dir(app::path());

    cmd
}

fn get_output(args: &[&str]) -> Result<String> {
    let mut cmd = construct_command(args);

    Ok(String::from_utf8(cmd.output()?.stdout)?)
}

fn is_warning_line(line: &str) -> bool {
    line.starts_with("warning: ") || line.contains("original line endings")
}
