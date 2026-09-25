use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, Result, bail};
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
    let mut prepared = Vec::new();
    let mut failures = Vec::new();
    for path in files_in(dest_root)? {
        let relative = path
            .strip_prefix(dest_root)
            .context("vendored path is outside the destination")?;
        if relative == Path::new("README.md") {
            continue;
        }
        match fs::read(source_root.join(relative)) {
            Ok(contents) => prepared.push((path, normalize(&contents))),
            Err(error) => failures.push((relative.to_path_buf(), error)),
        }
    }
    if !failures.is_empty() {
        failures.sort_by(|(left, _), (right, _)| left.cmp(right));
        let detail = failures
            .iter()
            .map(|(path, error)| format!("{}: {error}", path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        bail!("Could not read vendored files:\n{detail}");
    }
    for (dest, contents) in prepared {
        info!("Updating {}", dest.display());
        fs::write(&dest, contents)
            .with_context(|| format!("Could not write {}", dest.display()))?;
    }
    Ok(())
}

/// Remove trailing spaces from each line, matching `vdev check fmt`, and end
/// every line with a newline.
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
        assert_eq!(normalize(b"proto"), b"proto\n");
        assert_eq!(normalize(b"proto\n"), b"proto\n");
        assert_eq!(normalize(b"kept \ntrimmed  \r\n"), b"kept\ntrimmed\n");
    }

    #[test]
    fn leaves_the_destination_unchanged_when_a_source_file_is_missing() {
        let directory = TempDir::new().unwrap();
        let source_root = directory.path().join("source");
        let dest_root = directory.path().join("dest");
        fs::create_dir(&source_root).unwrap();
        fs::create_dir(&dest_root).unwrap();
        fs::write(source_root.join("kept.proto"), b"new\n").unwrap();
        fs::write(dest_root.join("kept.proto"), b"old\n").unwrap();
        fs::write(dest_root.join("gone.proto"), b"old\n").unwrap();

        let error = copy_existing_files(&source_root, &dest_root).unwrap_err();
        assert!(error.to_string().contains("gone.proto"), "{error}");
        assert_eq!(fs::read(dest_root.join("kept.proto")).unwrap(), b"old\n");
        assert_eq!(fs::read(dest_root.join("gone.proto")).unwrap(), b"old\n");
    }
}
