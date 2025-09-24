//! [`Glob`] paths provider.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub use glob::MatchOptions;
use glob::Pattern;
use tokio::task::spawn_blocking;
use tracing::error;

use super::PathsProvider;
use crate::FileSourceInternalEvents;

/// A glob-based path provider.
///
/// Provides the paths to the files on the file system that match include
/// patterns and don't match the exclude patterns.
#[derive(Clone)]
pub struct Glob<E: FileSourceInternalEvents + Clone> {
    include_patterns: Vec<String>,
    exclude_patterns: Vec<Pattern>,
    glob_match_options: MatchOptions,
    emitter: E,
}

impl<E: FileSourceInternalEvents> Glob<E> {
    /// Create a new [`Glob`].
    ///
    /// Returns `None` if patterns aren't valid.
    pub fn new(
        include_patterns: &[PathBuf],
        exclude_patterns: &[PathBuf],
        glob_match_options: MatchOptions,
        emitter: E,
    ) -> Option<Self> {
        let include_patterns = include_patterns
            .iter()
            .map(|path| path.to_str().map(ToOwned::to_owned))
            .collect::<Option<_>>()?;

        let exclude_patterns = exclude_patterns
            .iter()
            .filter_map(|path| path.to_str().map(|path| Pattern::new(path).ok()))
            .collect::<Option<Vec<_>>>()?;

        Some(Self {
            include_patterns,
            exclude_patterns,
            glob_match_options,
            emitter,
        })
    }
}

impl<E: FileSourceInternalEvents + Clone + Send + 'static> PathsProvider for Glob<E> {
    type IntoIter = Vec<PathBuf>;

    fn paths(
        &self,
        _should_glob: bool,
    ) -> Pin<Box<dyn Future<Output = Self::IntoIter> + Send + '_>> {
        Box::pin(async move {
            // Clone the data we need to move into the spawn_blocking task
            let include_patterns = self.include_patterns.clone();
            let exclude_patterns = self.exclude_patterns.clone();
            let glob_match_options = self.glob_match_options;
            let emitter = self.emitter.clone();

            // Use spawn_blocking to run the glob operations in a separate thread
            // This prevents blocking the async runtime with potentially expensive
            // filesystem operations
            match spawn_blocking(move || {
                include_patterns
                    .iter()
                    .flat_map(|include_pattern| {
                        glob::glob_with(include_pattern.as_str(), glob_match_options)
                            .expect("failed to read glob pattern")
                            .filter_map(|val| {
                                val.map_err(|error| {
                                    emitter.emit_path_globbing_failed(error.path(), error.error())
                                })
                                .ok()
                            })
                    })
                    .filter(|candidate_path: &PathBuf| -> bool {
                        !exclude_patterns.iter().any(|exclude_pattern| {
                            let candidate_path_str = candidate_path.to_str().unwrap();
                            exclude_pattern.matches(candidate_path_str)
                        })
                    })
                    .collect()
            })
            .await
            {
                Ok(paths) => paths,
                Err(e) => {
                    error!(message = "Error during glob scan", error = ?e);
                    Vec::new()
                }
            }
        })
    }
}
