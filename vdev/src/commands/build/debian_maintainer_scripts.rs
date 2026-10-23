use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

use crate::utils::paths;

/// Stage Debian maintainer scripts with the packaged configuration embedded.
#[derive(clap::Args, Debug)]
pub(super) struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let root = paths::find_repo_root()?;
        let destination = staging_directory(&root)?;
        generate(&root.join("distribution/debian"), &destination)
    }
}

// Only this dedicated output directory may be replaced. Reject symlinks so an
// existing target directory cannot redirect cleanup outside the repository.
fn staging_directory(root: &Path) -> Result<PathBuf> {
    let target = root.join("target");
    let destination = target.join("debian-maintainer-scripts");
    for path in [&target, &destination] {
        match fs::symlink_metadata(path) {
            Ok(metadata) => ensure!(
                metadata.file_type().is_dir(),
                "{} must be a directory, not a symlink or file",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("Checking Debian staging directory"),
        }
    }
    Ok(destination)
}

fn render(preinst: &str, stub: &str) -> Result<String> {
    const MARKER: &str = "{{config_stub~}}";
    ensure!(
        preinst.matches(MARKER).count() == 1,
        "preinst must contain exactly one config-stub placeholder"
    );
    ensure!(
        stub.ends_with('\n') && !stub.lines().any(|line| line == "VECTOR_CONFIG_STUB"),
        "config stub must end with a newline and not close its heredoc"
    );
    let mut handlebars = handlebars::Handlebars::new();
    // This is shell/YAML content, not HTML; preserve the stub byte-for-byte.
    // The template trims its own following newline; the stub supplies that newline.
    handlebars.register_escape_fn(handlebars::no_escape);
    handlebars.set_strict_mode(true);
    Ok(handlebars.render_template(preinst, &serde_json::json!({ "config_stub": stub }))?)
}

fn generate(source: &Path, destination: &Path) -> Result<()> {
    let stub =
        fs::read_to_string(source.join("vector.yaml")).context("Reading Debian config stub")?;
    let preinst = fs::read_to_string(source.join("scripts/preinst"))
        .context("Reading Debian preinst template")?;
    let rendered = render(&preinst, &stub)?;
    // cargo-deb reads this directory directly, so scripts removed from the source
    // must not survive a subsequent packaging run.
    match fs::remove_dir_all(destination) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("Clearing staged Debian maintainer scripts"),
    }
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
        for stub in [
            "# <>&\" {{literal}}\n",
            "# first\n",
            "# quotes '$' and backslashes \\\n\n",
        ] {
            assert_eq!(
                render("before\n{{config_stub~}}\nafter\n", stub).unwrap(),
                format!("before\n{stub}after\n")
            );
        }
    }

    #[test]
    fn rejects_invalid_inputs() {
        for (template, stub) in [
            ("no marker\n", "# stub\n"),
            ("{{config_stub~}}{{config_stub~}}", "# stub\n"),
            ("{{config_stub~}}", "# no newline"),
            ("{{config_stub~}}", "VECTOR_CONFIG_STUB\n"),
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
        fs::write(scripts.join("preinst"), "{{config_stub~}}").unwrap();
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

    #[test]
    #[cfg(unix)]
    fn removes_obsolete_scripts_on_regeneration() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let scripts = source.join("scripts");
        let output = temp.path().join("output");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(source.join("vector.yaml"), "# stub\n").unwrap();
        fs::write(scripts.join("preinst"), "{{config_stub~}}").unwrap();
        fs::write(scripts.join("prerm"), "#!/bin/sh\n").unwrap();
        generate(&source, &output).unwrap();
        assert!(output.join("prerm").exists());

        fs::remove_file(scripts.join("prerm")).unwrap();
        generate(&source, &output).unwrap();
        assert!(!output.join("prerm").exists());
        assert_eq!(
            fs::read_to_string(output.join("preinst")).unwrap(),
            "# stub\n"
        );
    }

    #[test]
    #[cfg(unix)]
    fn confines_staging_to_repository_directory() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        let destination = target.join("debian-maintainer-scripts");
        fs::write(outside.path().join("keep"), "unrelated data").unwrap();
        assert_eq!(staging_directory(root.path()).unwrap(), destination);

        symlink(outside.path(), &target).unwrap();
        assert!(staging_directory(root.path()).is_err());
        fs::remove_file(&target).unwrap();
        fs::create_dir(&target).unwrap();
        symlink(outside.path(), &destination).unwrap();
        assert!(staging_directory(root.path()).is_err());
        fs::remove_file(&destination).unwrap();
        fs::write(&destination, "unrelated file").unwrap();
        assert!(staging_directory(root.path()).is_err());
        assert_eq!(
            fs::read_to_string(outside.path().join("keep")).unwrap(),
            "unrelated data"
        );
    }
}
