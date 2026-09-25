use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, Result};
use regex::Regex;
use semver::Version;
use tempfile::TempDir;

use crate::app::{self, CommandExt as _};

pub(super) struct VendorSpec {
    pub(super) repo_url: &'static str,
    pub(super) dest: &'static str,
    pub(super) src_rel: &'static str,
    pub(super) tag_pattern: &'static str,
    pub(super) title: &'static str,
    pub(super) command: &'static str,
}

pub(super) fn refresh(spec: &VendorSpec, tag: Option<&str>) -> Result<()> {
    app::set_repo_dir()?;

    let pattern = Regex::new(spec.tag_pattern)
        .with_context(|| format!("invalid tag pattern {}", spec.tag_pattern))?;
    let tag = match tag {
        Some(tag) => tag.to_string(),
        None => highest_release_tag(spec.repo_url, &pattern)?,
    };
    info!("Vendoring {tag} from {}", spec.repo_url);

    let checkout = TempDir::new().context("Could not create a temporary directory")?;
    Command::new("git")
        .args([
            "-c",
            "advice.detachedHead=false",
            "clone",
            "--depth",
            "1",
            "--branch",
            &tag,
            "--filter",
            "blob:none",
            spec.repo_url,
        ])
        .arg(checkout.path())
        .check_run()
        .with_context(|| format!("Could not clone {} at {tag}", spec.repo_url))?;

    let source_root = checkout.path().join(spec.src_rel);
    let dest_root = PathBuf::from(spec.dest);
    copy_existing_files(&source_root, &dest_root)?;

    fs::write(dest_root.join("README.md"), readme(spec, &tag))?;
    Ok(())
}

fn highest_release_tag(repo: &str, pattern: &Regex) -> Result<String> {
    let output = Command::new("git")
        .args(["ls-remote", "--tags", "--refs", repo])
        .check_output()
        .with_context(|| format!("Could not list tags in {repo}"))?;
    let tags = tags_from_ls_remote(&output);
    select_highest_tag(tags, pattern)
        .with_context(|| format!("Could not select a release from {repo}"))
}

fn tags_from_ls_remote(output: &str) -> impl Iterator<Item = &str> {
    output.lines().filter_map(|line| {
        let (_object, reference) = line.split_once('\t')?;
        reference.strip_prefix("refs/tags/")
    })
}

fn select_highest_tag<'a>(
    tags: impl IntoIterator<Item = &'a str>,
    pattern: &Regex,
) -> Result<String> {
    let mut highest: Option<(Version, &str)> = None;
    for tag in tags.into_iter().filter(|tag| pattern.is_match(tag)) {
        let version = version_of(tag)?;
        if highest
            .as_ref()
            .is_none_or(|(current, _)| version > *current)
        {
            highest = Some((version, tag));
        }
    }
    highest
        .map(|(_, tag)| tag.to_string())
        .with_context(|| format!("no tag matched the release pattern {}", pattern.as_str()))
}

fn version_of(tag: &str) -> Result<Version> {
    let bare = tag.strip_prefix('v').unwrap_or(tag);
    Version::parse(bare).with_context(|| format!("release tag {tag} is not a semantic version"))
}

fn copy_existing_files(source_root: &Path, dest_root: &Path) -> Result<()> {
    for path in files_in(dest_root)? {
        let relative = path
            .strip_prefix(dest_root)
            .context("vendored path is outside the destination")?;
        if relative == Path::new("README.md") {
            continue;
        }
        info!("Updating {}", path.display());
        copy(&source_root.join(relative), &path)?;
    }
    Ok(())
}

/// Read `source` and write it to `dest`.
///
/// Trailing spaces are removed from each line, matching `vdev check fmt`, and
/// every line is written with a newline.
fn copy(source: &Path, dest: &Path) -> Result<()> {
    let contents =
        fs::read(source).with_context(|| format!("Could not read {}", source.display()))?;
    fs::write(dest, normalize(&contents))
        .with_context(|| format!("Could not write {}", dest.display()))
}

fn normalize(contents: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(contents.len() + 1);
    let mut lines = contents.split(|byte| *byte == b'\n').peekable();
    while let Some(mut line) = lines.next() {
        // A file that already ends in a newline yields an empty final segment.
        // Skipping that segment leaves exactly one newline per line.
        if line.is_empty() && lines.peek().is_none() {
            break;
        }
        if let Some(stripped) = line.strip_suffix(b"\r") {
            line = stripped;
        }
        while line.ends_with(b" ") {
            line = &line[..line.len() - 1];
        }
        normalized.extend_from_slice(line);
        normalized.push(b'\n');
    }
    normalized
}

fn files_in(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("Could not read {}", directory.display()))?
        {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            if file_type.is_dir() {
                directories.push(path);
            } else if file_type.is_file() {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn readme(spec: &VendorSpec, tag: &str) -> String {
    let slug = spec
        .repo_url
        .strip_prefix("https://github.com/")
        .unwrap_or(spec.repo_url);
    let VendorSpec {
        title,
        repo_url: repo,
        src_rel,
        command,
        ..
    } = spec;
    indoc::formatdoc! {"
        # {title}

        Vendored from [`{slug}`]({repo})
        [`{src_rel}`]({repo}/tree/{tag}/{src_rel})
        at release [`{tag}`]({repo}/releases/tag/{tag}).

        Files already present in this directory are refreshed from that release.

        Refresh by running `cargo vdev {command}` for the latest release,
        or `cargo vdev {command} {tag}` to pin this tag again.
    "}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_highest_stable_tag() {
        let pattern = Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+$").unwrap();
        let tag = select_highest_tag(
            ["v1.9.0", "v1.10.0", "v1.2.0", "v1.10.0-rc.1", "main"],
            &pattern,
        )
        .unwrap();
        assert_eq!(tag, "v1.10.0");
    }

    #[test]
    fn selects_highest_numeric_tag() {
        let pattern = Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$").unwrap();
        let tag =
            select_highest_tag(["7.9.0", "7.10.0", "7.83.3", "7.83.3-rc.1"], &pattern).unwrap();
        assert_eq!(tag, "7.83.3");
    }

    #[test]
    fn parses_ls_remote_tags() {
        let output = "abc\trefs/tags/v1.1.0\ndef\trefs/tags/v1.1.0^{}\n";
        let tags: Vec<_> = tags_from_ls_remote(output).collect();
        assert_eq!(tags, ["v1.1.0", "v1.1.0^{}"]);
    }

    #[test]
    fn adds_a_missing_trailing_newline() {
        let directory = TempDir::new().unwrap();
        let source = directory.path().join("source.txt");
        let dest = directory.path().join("dest.txt");

        fs::write(&source, b"proto").unwrap();
        copy(&source, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"proto\n");

        fs::write(&source, b"proto\n").unwrap();
        copy(&source, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"proto\n");

        fs::write(&source, b"kept \ntrimmed  \r\n").unwrap();
        copy(&source, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"kept\ntrimmed\n");
    }
}
