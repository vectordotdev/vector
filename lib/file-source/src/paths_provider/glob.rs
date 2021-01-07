//! [`Glob`] paths provider.

use super::PathsProvider;
use glob::Pattern;
use globwalk::glob;
use std::path::PathBuf;

#[derive(Debug)]
/// An error that arised either during parsing or execution of this glob.
pub enum GlobError {
    /// Include glob pattern could not be parsed.
    InvalidIncludePattern(globwalk::GlobError),
    /// Exclude glob pattern could not be parsed.
    InvalidExcludePattern(glob::PatternError),
    /// Failed while iterating on the glob.
    WalkError(globwalk::WalkError),
}

/// A glob-based path provider.
///
/// Provides the paths to the files on the file system that match include
/// patterns and don't match the exclude patterns.
pub struct Glob {
    include_patterns: Vec<String>,
    exclude_patterns: Vec<Pattern>,
}

impl Glob {
    /// Create a new [`Glob`].
    ///
    /// Returns `GlobError` if any of the patterns is not valid.
    pub fn new(
        include_patterns: &[String],
        exclude_patterns: &[String],
    ) -> Result<Self, GlobError> {
        // Validate include patterns. We can't parse the `GlobWalkers` and save them in our struct
        // for later use because they are mutable iterators and don't implement the
        // `std::clone::Clone` trait.
        include_patterns
            .iter()
            .map(glob)
            .collect::<Result<Vec<_>, _>>()
            .map_err(GlobError::InvalidIncludePattern)?;

        let include_patterns = include_patterns.to_owned();

        let exclude_patterns = exclude_patterns
            .iter()
            .map(AsRef::as_ref)
            .map(Pattern::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(GlobError::InvalidExcludePattern)?;

        Ok(Self {
            include_patterns,
            exclude_patterns,
        })
    }
}

impl PathsProvider for Glob {
    type IntoIter = Vec<PathBuf>;
    type Error = GlobError;

    fn paths(&self) -> Result<Self::IntoIter, Self::Error> {
        let mut paths = Vec::new();

        for include_pattern in &self.include_patterns {
            let glob = glob(include_pattern).map_err(GlobError::InvalidIncludePattern)?;

            for dir_entry in glob {
                let path = dir_entry.map_err(GlobError::WalkError)?.into_path();
                let is_excluded = self.exclude_patterns.iter().any(|exclude_pattern| {
                    path.to_str()
                        .map_or(false, |path| exclude_pattern.matches(path))
                });

                if is_excluded {
                    continue;
                }

                paths.push(path);
            }
        }

        Ok(paths)
    }
}
