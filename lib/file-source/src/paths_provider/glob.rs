//! [`Glob`] paths provider.

use super::PathsProvider;

use glob::Pattern;
use std::path::PathBuf;

pub use glob::MatchOptions;

/// A glob-based path provider.
///
/// Provides the paths to the files on the file system that match include
/// patterns and don't match the exclude patterns.
pub struct Glob {
    include_patterns: Vec<String>,
    exclude_patterns: Vec<Pattern>,
    glob_match_options: MatchOptions,
}

impl Glob {
    /// Create a new [`Glob`].
    ///
    /// Returns `None` if patterns aren't valid.
    pub fn new(
        include_patterns: &[PathBuf],
        exclude_patterns: &[PathBuf],
        glob_match_options: MatchOptions,
    ) -> Option<Self> {
        let include_patterns = include_patterns
            .iter()
            .map(|path| path.to_str().map(ToOwned::to_owned))
            .collect::<Option<_>>()?;

        let exclude_patterns = exclude_patterns
            .iter()
            .map(|path| path.to_str().map(|path| Pattern::new(path).ok()))
            .flatten()
            .collect::<Option<Vec<_>>>()?;

        Some(Self {
            include_patterns,
            exclude_patterns,
            glob_match_options,
        })
    }
}

impl PathsProvider for Glob {
    type IntoIter = Vec<PathBuf>;

    fn paths(&self) -> Self::IntoIter {
        self.include_patterns
            .iter()
            .flat_map(|include_pattern| {
                glob::glob_with(include_pattern.as_str(), &self.glob_match_options)
                    .expect("failed to read glob pattern")
                    .filter_map(|val| val.ok())
            })
            .filter(|candidate_path: &PathBuf| -> bool {
                !self.exclude_patterns.iter().any(|exclude_pattern| {
                    let candidate_path_str = candidate_path.to_str().unwrap();
                    exclude_pattern.matches(candidate_path_str)
                })
            })
            .collect()
    }
}
