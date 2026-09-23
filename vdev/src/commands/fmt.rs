use anyhow::Result;

use crate::{
    app,
    commands::style,
    utils::{git::git_ls_files, paths::prettier},
};

pub(crate) const PRETTIER_EXTENSIONS: &[&str] =
    &["*.yml", "*.yaml", "*.js", "*.ts", "*.tsx", "*.json"];

/// Vendored protocol trees. Prettier reformats their YAML indentation, and it
/// formats a path given on the command line even when `.prettierignore` lists it.
const VENDORED_PROTO_PREFIXES: &[&str] = &[
    "lib/opentelemetry-proto/src/proto/opentelemetry-proto/",
    "lib/datadog-proto/proto/datadog/trace/",
];

pub(crate) fn files_for_prettier(extension: &str) -> Result<Vec<String>> {
    Ok(git_ls_files(Some(extension))?
        .into_iter()
        .filter(|path| !is_vendored_proto(path))
        .collect())
}

fn is_vendored_proto(path: &str) -> bool {
    VENDORED_PROTO_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

/// Apply format changes across the repository
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;
        style::fix_changed()?;

        info!("Formatting Rust code...");
        app::exec("cargo", ["fmt", "--all"], true)?;

        for ext in PRETTIER_EXTENSIONS {
            let files = files_for_prettier(ext)?;
            if files.is_empty() {
                continue;
            }
            info!("Formatting {ext} files with prettier...");
            let args = ["--write"]
                .into_iter()
                .chain(files.iter().map(String::as_str));
            prettier(args, true)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::is_vendored_proto;

    #[test]
    fn skips_vendored_protocol_files() {
        assert!(is_vendored_proto(
            "lib/opentelemetry-proto/src/proto/opentelemetry-proto/opentelemetry/proto/collector/logs/v1/logs_service_http.yaml"
        ));
        assert!(is_vendored_proto(
            "lib/datadog-proto/proto/datadog/trace/span.proto"
        ));
        assert!(!is_vendored_proto("config/vector.yaml"));
    }
}
