use std::{collections::HashSet, process::Command, process::Output};

use anyhow::{anyhow, Result, Context};

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

/// Get a list of files that have been modified, as a vector of strings
pub fn get_modified_files() -> Result<Vec<String>> {
    let args = vec![
        "ls-files",
        "--full-name",
        "--modified",
        "--others",
        "--exclude-standard",
    ];
    Ok(capture_output(&args)?.lines().map(str::to_owned).collect())
}

pub fn set_config_values(config_values: &[(&str, &str)]) -> Result<String> {
    let mut args = vec!["config"];

    for (key, value) in config_values {
        args.push(key);
        args.push(value);
    }

    capture_output(&args)
}

/// Checks if the current directory's repo is clean
pub fn check_git_repository_clean() -> Result<bool> {
    Ok(Command::new("git")
        .args(["diff-index", "--quiet", "HEAD"])
        .stdout(std::process::Stdio::null())
        .status()
        .map(|status| status.success())?)
}

/// Commits changes from the current repo
pub fn commit(commit_message: &str) -> Result<Output> {
    let output = run_command_check_status(&["-am", commit_message])?;
    Ok(output)
}

/// Pushes changes from the current repo
pub fn push() -> Result<Output> {
    let output = Command::new("git")
        .arg("push")
        .output()
        .map_err(|e| anyhow!("{}", e))?;

    Ok(output)
}

pub fn clone(repo_url: &str) -> Result<Output> {
    let output = Command::new("git")
        .arg("clone")
        .arg(repo_url)
        .output()
        .map_err(|e| anyhow!("{}", e))?;

    Ok(output)
}

fn capture_output(args: &[&str]) -> Result<String> {
    Command::new("git").in_repo().args(args).capture_output()
}

// TODO: Potentially modify capture_output function with this implementation if it satisfies the use case
fn run_command_check_status(args: &[&str]) -> Result<Output> {
    let mut command = Command::new("git");
    command.args(args);

    let output = command.output().context(format!("Failed to run command: git {:?}", args))?;
    let status = output.status;
    if !status.success() {
        return Err(anyhow!("Command failed with exit code: {status}"));
    }

    Ok(output)
}


fn is_warning_line(line: &str) -> bool {
    line.starts_with("warning: ") || line.contains("original line endings")
}
