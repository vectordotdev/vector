use std::{fs, sync::LazyLock};

use anyhow::{Context as _, Result, bail};
use glob::Pattern;

use crate::utils::git;

static IGNORED_PATHS: LazyLock<Vec<Pattern>> = LazyLock::new(|| {
    [
        "*png",
        "*svg",
        "*gif",
        "*ico",
        "*sig",
        "*html",
        "*desc",
        "tests/data*",
        "*avsc",
        "*avro",
        "*pb",
    ]
    .into_iter()
    .map(|pattern| Pattern::new(pattern).expect("style ignore pattern should be valid"))
    .collect()
});

#[derive(Debug, Default, PartialEq, Eq)]
struct Issues {
    missing_newline: bool,
    crlf: bool,
    trailing_spaces: Vec<usize>,
}

pub(crate) fn check_all() -> Result<()> {
    check(git::list_files()?)
}

pub(crate) fn fix_changed() -> Result<()> {
    let mut files = git::changed_files()?;
    if files.is_empty() {
        files = git::list_files()?;
    }

    let files = eligible_files(files);
    info!("Fixing style in {} files...", files.len());

    for file in files {
        let contents = fs::read(&file).with_context(|| format!("Could not read {file}"))?;
        let Some(fixed) = fix_text(&contents) else {
            continue;
        };
        if contents != fixed {
            fs::write(&file, fixed).with_context(|| format!("Could not update {file}"))?;
        }
    }

    Ok(())
}

fn check(files: Vec<String>) -> Result<()> {
    let files = eligible_files(files);
    info!("Checking style in {} files...", files.len());

    let mut failed = false;
    for file in files {
        let contents = fs::read(&file).with_context(|| format!("Could not read {file}"))?;
        if is_binary(&contents) {
            continue;
        }
        let issues = inspect(&contents);

        if issues.missing_newline {
            println!("File \"{file}\" doesn't end with a newline");
            failed = true;
        }
        if issues.crlf {
            println!("File \"{file}\" contains CRLF line breaks instead of LF line breaks");
            failed = true;
        }
        if !issues.trailing_spaces.is_empty() {
            let lines = issues
                .trailing_spaces
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            println!("File \"{file}\" contains trailing spaces on line(s): {lines}");
            failed = true;
        }
    }

    if failed {
        bail!("style checks failed");
    }

    Ok(())
}

fn eligible_files(files: Vec<String>) -> Vec<String> {
    files
        .into_iter()
        .filter(|file| {
            !IGNORED_PATHS.iter().any(|pattern| pattern.matches(file))
                && fs::symlink_metadata(file).is_ok_and(|metadata| metadata.file_type().is_file())
        })
        .collect()
}

fn inspect(contents: &[u8]) -> Issues {
    let mut issues = Issues {
        missing_newline: !contents.is_empty() && !contents.ends_with(b"\n"),
        ..Issues::default()
    };

    for (index, line) in contents.split_inclusive(|byte| *byte == b'\n').enumerate() {
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        let line = if let Some(line) = line.strip_suffix(b"\r") {
            issues.crlf = true;
            line
        } else {
            line
        };

        if line.ends_with(b" ") {
            issues.trailing_spaces.push(index + 1);
        }
    }

    issues
}

fn is_binary(contents: &[u8]) -> bool {
    contents.contains(&b'\0') || std::str::from_utf8(contents).is_err()
}

fn fix_text(contents: &[u8]) -> Option<Vec<u8>> {
    (!is_binary(contents)).then(|| fix(contents))
}

fn fix(contents: &[u8]) -> Vec<u8> {
    let was_nonempty = !contents.is_empty();
    let mut fixed = Vec::with_capacity(contents.len() + 1);

    for line in contents.split_inclusive(|byte| *byte == b'\n') {
        let has_newline = line.ends_with(b"\n");
        let mut line = if has_newline {
            &line[..line.len() - 1]
        } else {
            line
        };

        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        while line.ends_with(b" ") {
            line = &line[..line.len() - 1];
        }

        fixed.extend_from_slice(line);
        if has_newline {
            fixed.push(b'\n');
        }
    }

    if was_nonempty && !fixed.ends_with(b"\n") {
        fixed.push(b'\n');
    }

    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_style_issues() {
        assert_eq!(
            inspect(b"clean\r\ntrailing \nmissing newline"),
            Issues {
                missing_newline: true,
                crlf: true,
                trailing_spaces: vec![2],
            }
        );
    }

    #[test]
    fn fixes_style_issues_in_one_pass() {
        assert_eq!(
            fix(b"windows  \r\ntrailing  \nmissing newline"),
            b"windows\ntrailing\nmissing newline\n"
        );
    }

    #[test]
    fn preserves_empty_and_clean_files() {
        assert_eq!(fix(b""), b"");
        assert_eq!(fix(b"clean\n"), b"clean\n");
        assert_eq!(fix(b"   "), b"\n");
        assert_eq!(inspect(b"clean\n"), Issues::default());
    }

    #[test]
    fn excludes_binary_content_from_fixes() {
        assert_eq!(fix_text(b"binary\0payload "), None);
        assert_eq!(fix_text(&[0xff, b' ']), None);
    }

    #[cfg(unix)]
    #[test]
    fn excludes_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target");
        let link = directory.path().join("link");
        fs::write(&target, "target").unwrap();
        symlink(&target, &link).unwrap();

        assert!(
            eligible_files(vec![link.to_string_lossy().into_owned()]).is_empty(),
            "symlinks must not be treated as writable files"
        );
        assert_eq!(fs::read_to_string(target).unwrap(), "target");
    }

    #[test]
    fn ignores_configured_paths() {
        for path in [
            "image.png",
            "image.svg",
            "image.gif",
            "image.ico",
            "artifact.sig",
            "page.html",
            "schema.desc",
            "tests/data/example.txt",
            "schema.avsc",
            "events.avro",
            "message.pb",
        ] {
            assert!(
                IGNORED_PATHS.iter().any(|pattern| pattern.matches(path)),
                "{path} should be ignored"
            );
        }
    }
}
