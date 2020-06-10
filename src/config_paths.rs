use glob::glob;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use std::path::PathBuf;

lazy_static! {
    pub static ref DEFAULT_CONFIG_PATHS: Vec<PathBuf> = vec!["/etc/vector/vector.toml".into()];
}

pub static CONFIG_PATHS: OnceCell<Vec<PathBuf>> = OnceCell::new();

/// Expands, dedups, and sets global values.
pub fn prepare(paths: Vec<PathBuf>) -> Option<Vec<PathBuf>> {
    let mut config_paths = expand(paths)?;
    config_paths.sort();
    config_paths.dedup();
    CONFIG_PATHS
        .set(config_paths.clone())
        .expect("Cannot set global config paths");
    Some(config_paths)
}

/// Expand a list of paths (potentially containing glob patterns) into real
/// config paths, replacing it with the default paths when empty.
pub fn expand(config_paths: Vec<PathBuf>) -> Option<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for config_pattern in if !config_paths.is_empty() {
        config_paths
    } else {
        DEFAULT_CONFIG_PATHS.to_vec()
    } {
        let matches: Vec<PathBuf> = match glob(config_pattern.to_str().expect("No ability to glob"))
        {
            Ok(glob_paths) => glob_paths.filter_map(Result::ok).collect(),
            Err(err) => {
                error!(message = "Failed to read glob pattern.", path = ?config_pattern, error = ?err);
                return None;
            }
        };

        if matches.is_empty() {
            error!(message = "Config file not found in path.", path = ?config_pattern);
            std::process::exit(exitcode::CONFIG);
        }

        for path in matches {
            paths.push(path);
        }
    }
    Some(paths)
}
