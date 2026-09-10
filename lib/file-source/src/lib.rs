#![deny(warnings)]
#![deny(clippy::all)]

pub mod file_server;
pub mod file_watcher;
pub mod notify_watcher;
pub mod paths_provider;

/// Make `path` absolute the same way the `notify` crate does internally before using a path
/// passed to `watch()` -- joined onto `cwd` if relative, unchanged if already absolute. Needed
/// wherever a path from a glob-based `PathsProvider`/`include` pattern (which can be relative) is
/// compared against a path `notify` reports in an event (always absolute). `cwd` is `None` only
/// if `std::env::current_dir()` itself failed, in which case `path` is returned unchanged.
pub(crate) fn absolutize(
    path: &std::path::Path,
    cwd: Option<&std::path::Path>,
) -> std::path::PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    match cwd {
        Some(cwd) => cwd.join(path),
        None => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::absolutize;

    #[test]
    fn leaves_absolute_paths_unchanged() {
        let cwd = PathBuf::from("/home/user/project");
        let absolute = PathBuf::from("/var/log/app.log");
        assert_eq!(absolutize(&absolute, Some(&cwd)), absolute);
    }

    #[test]
    fn joins_relative_paths_onto_cwd() {
        let cwd = PathBuf::from("/home/user/project");
        let relative = PathBuf::from("logs/app.log");
        assert_eq!(
            absolutize(&relative, Some(&cwd)),
            PathBuf::from("/home/user/project/logs/app.log")
        );
    }

    #[test]
    fn falls_back_to_the_relative_path_when_cwd_is_unknown() {
        let relative = PathBuf::from("logs/app.log");
        assert_eq!(absolutize(&relative, None), relative);
    }
}
