use glob::glob;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use std::path::PathBuf;

pub mod watcher;

lazy_static! {
    pub static ref DEFAULT_CONFIG_PATHS: Vec<PathBuf> = vec!["/etc/vector/vector.toml".into()];
}

pub static CONFIG_PATHS: OnceCell<Vec<PathBuf>> = OnceCell::new();

/// Expand a list of paths (potentially containing glob patterns) into real
/// config paths, replacing it with the default paths when empty.
pub fn process_paths(config_paths: &[PathBuf]) -> Option<Vec<PathBuf>> {
    let starting_paths = if !config_paths.is_empty() {
        config_paths
    } else {
        &DEFAULT_CONFIG_PATHS
    };

    let mut paths = Vec::new();

    for config_pattern in starting_paths {
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

    paths.sort();
    paths.dedup();
    CONFIG_PATHS
        .set(paths.clone())
        .expect("Cannot set global config paths");

    Some(paths)
}
