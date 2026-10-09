use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

use crate::utils::paths;

/// Stage Debian maintainer scripts with the packaged configuration embedded.
#[derive(clap::Args, Debug)]
pub(super) struct Cli {
    /// Directory where cargo-deb reads the generated scripts.
    destination: PathBuf,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        generate(
            &paths::find_repo_root()?.join("distribution/debian"),
            &self.destination,
        )
    }
}

fn render(preinst: &str, stub: &str) -> Result<String> {
    const MARKER: &str = "@VECTOR_CONFIG_STUB@\n";
    ensure!(
        preinst.matches(MARKER).count() == 1,
        "preinst must contain exactly one config-stub placeholder"
    );
    ensure!(
        stub.ends_with('\n') && !stub.lines().any(|line| line == "VECTOR_CONFIG_STUB"),
        "config stub must end with a newline and not close its heredoc"
    );
    Ok(preinst.replace(MARKER, stub))
}

fn generate(source: &Path, destination: &Path) -> Result<()> {
    let stub =
        fs::read_to_string(source.join("vector.yaml")).context("Reading Debian config stub")?;
    let preinst = fs::read_to_string(source.join("scripts/preinst"))
        .context("Reading Debian preinst template")?;
    let rendered = render(&preinst, &stub)?;
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source.join("scripts"))? {
        let entry = entry?;
        fs::copy(entry.path(), destination.join(entry.file_name()))?;
    }
    fs::write(destination.join("preinst"), rendered)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_literal_content() {
        for stub in ["# first\n", "# quotes '$' and backslashes \\\n\n"] {
            assert_eq!(
                render("before\n@VECTOR_CONFIG_STUB@\nafter\n", stub).unwrap(),
                format!("before\n{stub}after\n")
            );
        }
    }

    #[test]
    fn rejects_invalid_inputs() {
        for (template, stub) in [
            ("no marker\n", "# stub\n"),
            ("@VECTOR_CONFIG_STUB@\n@VECTOR_CONFIG_STUB@\n", "# stub\n"),
            ("@VECTOR_CONFIG_STUB@\n", "# no newline"),
            ("@VECTOR_CONFIG_STUB@\n", "VECTOR_CONFIG_STUB\n"),
        ] {
            assert!(render(template, stub).is_err());
        }
    }

    #[test]
    #[cfg(unix)]
    fn stages_scripts_and_regenerates_stub() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let scripts = source.join("scripts");
        let output = temp.path().join("output");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(scripts.join("preinst"), "@VECTOR_CONFIG_STUB@\n").unwrap();
        fs::write(scripts.join("postinst"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(scripts.join("preinst"), fs::Permissions::from_mode(0o755)).unwrap();
        for stub in ["# first\n", "# changed\n"] {
            fs::write(source.join("vector.yaml"), stub).unwrap();
            generate(&source, &output).unwrap();
            assert_eq!(fs::read_to_string(output.join("preinst")).unwrap(), stub);
            assert_eq!(
                fs::read(output.join("postinst")).unwrap(),
                fs::read(scripts.join("postinst")).unwrap()
            );
            assert_eq!(
                fs::metadata(output.join("preinst"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
    }
}
