//! Lint YAML fenced code blocks inside Markdown files.
//!
//! Surfaces as `cargo vdev check yaml-in-markdown`.
//! The fix counterpart lives in `crate::commands::fmt::yaml_in_markdown`.

use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use serde_yaml::Value;

use crate::utils::git::git_ls_files;

// ---------------------------------------------------------------------------
// Regex constants
// ---------------------------------------------------------------------------

static FENCE_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i-u)^```ya?ml\b").unwrap());
static FENCE_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u)^```\s*$").unwrap());
static DIFF_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[+ -]").unwrap());
static DOC_SEPARATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u)(?m)^---\s*$").unwrap());

// ---------------------------------------------------------------------------
// YAML block extraction
// ---------------------------------------------------------------------------

/// A YAML fenced code block extracted from a Markdown file.
pub struct YamlBlock {
    /// 1-indexed first content line (the line after the opening fence).
    pub start_line: usize,
    /// 1-indexed last content line (the line before the closing fence, inclusive).
    pub end_line: usize,
    pub content: String,
}

/// Return all YAML fenced code blocks in `text`.
pub fn extract_yaml_blocks(text: &str) -> Vec<YamlBlock> {
    let mut blocks = Vec::new();
    let mut inside = false;
    let mut start_line = 0usize;
    let mut buf = String::new();

    for (i, line) in text.lines().enumerate() {
        let line_no = i + 1; // 1-indexed
        if !inside && FENCE_OPEN.is_match(line.trim()) {
            inside = true;
            start_line = line_no + 1; // content begins on the next line
            buf.clear();
        } else if inside && FENCE_CLOSE.is_match(line.trim()) {
            inside = false;
            let end_line = line_no - 1; // last content line
            blocks.push(YamlBlock {
                start_line,
                end_line,
                content: buf.clone(),
            });
        } else if inside {
            buf.push_str(line);
            buf.push('\n');
        }
    }

    blocks
}

// ---------------------------------------------------------------------------
// Diff-marker handling
// ---------------------------------------------------------------------------

/// If every non-empty line in `content` starts with a diff prefix (`+`, `-`,
/// or space) **and** at least one line starts with `+` or `-`, return the
/// "after" state: context and added lines with the prefix stripped; removed
/// lines (`-`) are dropped.
///
/// Returns `None` when the block is not a diff (including the all-spaces
/// case, which is ordinary YAML indentation).
pub fn strip_diff_markers(content: &str) -> Option<String> {
    if content.is_empty() {
        return None;
    }

    let mut has_diff_marker = false;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if !DIFF_LINE.is_match(line) {
            return None;
        }
        if matches!(line.chars().next(), Some('+' | '-')) {
            has_diff_marker = true;
        }
    }

    // A block where every line starts with a space is normal YAML, not a diff.
    if !has_diff_marker {
        return None;
    }

    let after: String = content
        .lines()
        .filter_map(|line| {
            if line.trim().is_empty() {
                Some(line)
            } else if matches!(line.chars().next(), Some(' ' | '+')) {
                Some(&line[1..]) // strip the leading marker character
            } else {
                None // drop '-' (removed) lines
            }
        })
        .flat_map(|line| [line, "\n"])
        .collect();

    Some(after)
}

// ---------------------------------------------------------------------------
// Fix logic
// ---------------------------------------------------------------------------

/// Round-trip `content` through `serde_yaml` to canonicalise formatting.
///
/// Multi-document blocks (separated by `---`) are fixed document-by-document
/// and then rejoined, matching the Python `explicit_start=False` behaviour
/// (the leading `---\n` that `serde_yaml::to_string` always prepends is
/// stripped from each part).
///
/// Returns `Err` if any document cannot be parsed.
pub fn fix_yaml_content(content: &str) -> Result<String> {
    // Split on document-separator lines ("---"), fix each part, rejoin.
    let parts: Vec<&str> = DOC_SEPARATOR.split(content).collect();

    let fixed_parts: Vec<String> = parts
        .iter()
        .map(|part| fix_yaml_document(part))
        .collect::<Result<_>>()?;

    let mut result = fixed_parts.join("---\n");

    // Ensure the result ends with a newline so the closing fence stays on its
    // own line.
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }

    Ok(result)
}

/// Round-trip a single YAML document through `serde_yaml`.
/// Strips the leading `---\n` that `serde_yaml::to_string` always prepends.
fn fix_yaml_document(doc: &str) -> Result<String> {
    let value: Value = serde_yaml::from_str(doc)?;
    let serialised = serde_yaml::to_string(&value)?;
    // serde_yaml always prepends "---\n"; strip it to match Python's
    // `explicit_start=False`.
    let stripped = serialised
        .strip_prefix("---\n")
        .unwrap_or(&serialised)
        .to_owned();
    Ok(stripped)
}

/// Fix YAML blocks in each file in `paths`, rewriting files in-place.
///
/// Blocks are processed in **reverse order by `start_line`** so earlier line
/// offsets remain valid after replacing later blocks.
///
/// Returns `true` if any file was modified.
pub fn fix_files(paths: &[String], verbose: bool) -> Result<bool> {
    let mut any_modified = false;

    for path in paths {
        let text = std::fs::read_to_string(path)?;
        let blocks = extract_yaml_blocks(&text);
        if blocks.is_empty() {
            continue;
        }

        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut file_modified = false;

        // Process in reverse so earlier line numbers stay valid.
        let indexed: Vec<(usize, YamlBlock)> =
            blocks.into_iter().enumerate().map(|(i, b)| (i + 1, b)).collect();

        for (idx, block) in indexed.into_iter().rev() {
            match fix_yaml_content(&block.content) {
                Ok(fixed) => {
                    if fixed == block.content {
                        if verbose {
                            println!("{path}: block {idx} (line {}) OK", block.start_line);
                        }
                        continue;
                    }

                    // Replace lines[start_line-1 .. end_line] with the fixed content.
                    let fixed_lines: Vec<String> = fixed.lines().map(str::to_owned).collect();
                    lines.splice(
                        (block.start_line - 1)..block.end_line,
                        fixed_lines,
                    );
                    file_modified = true;
                    println!("{path}: block {idx} (line {}) FIXED", block.start_line);
                }
                Err(err) => {
                    warn!(
                        "{path}: block {idx} (line {}) SKIPPED — could not parse: {err}",
                        block.start_line
                    );
                }
            }
        }

        if file_modified {
            // Rebuild the file, restoring newlines that `lines()` stripped.
            let mut output = lines.join("\n");
            if text.ends_with('\n') {
                output.push('\n');
            }
            std::fs::write(path, output)?;
            any_modified = true;
        }
    }

    Ok(any_modified)
}

// ---------------------------------------------------------------------------
// CLI entry point
// ---------------------------------------------------------------------------

/// Check YAML code blocks inside Markdown files
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Print OK status for passing blocks and files with no YAML blocks
    #[arg(long)]
    show_ok: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let files = git_ls_files(Some("*.md"))?;
        if files.is_empty() {
            return Ok(());
        }

        let mut had_failure = false;

        for path in &files {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(err) => {
                    warn!("ERROR: {path}: {err}");
                    had_failure = true;
                    continue;
                }
            };

            let blocks = extract_yaml_blocks(&text);
            if blocks.is_empty() {
                if self.show_ok {
                    println!("{path}: no YAML blocks found");
                }
                continue;
            }

            for (idx, block) in blocks.iter().enumerate().map(|(i, b)| (i + 1, b)) {
                let (lint_content, is_diff) = match strip_diff_markers(&block.content) {
                    Some(stripped) => (stripped, true),
                    None => (block.content.clone(), false),
                };

                let diff_suffix = if is_diff { " (diff)" } else { "" };

                match serde_yaml::from_str::<Value>(&lint_content) {
                    Ok(_) => {
                        if self.show_ok {
                            println!(
                                "{path}: block {idx} (line {}){diff_suffix} OK",
                                block.start_line
                            );
                        }
                    }
                    Err(err) => {
                        had_failure = true;
                        println!(
                            "{path}: block {idx} (line {}){diff_suffix} FAILED",
                            block.start_line
                        );
                        let md_line = err
                            .location()
                            .map(|loc| loc.line() + block.start_line - 1);
                        let col = err.location().map(|loc| loc.column());
                        match (md_line, col) {
                            (Some(line), Some(col)) => {
                                println!("  line {line}:{col}: [error] {err}");
                            }
                            _ => {
                                println!("  [error] {err}");
                            }
                        }
                    }
                }
            }
        }

        if had_failure {
            std::process::exit(1);
        }

        Ok(())
    }
}
