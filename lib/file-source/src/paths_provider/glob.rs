//! [`Glob`] paths provider.

use super::PathsProvider;
use glob::Pattern;
use globwalk::glob;
use std::path::PathBuf;

#[derive(Debug, snafu::Snafu)]
/// An error that arised either during parsing or execution of this glob.
pub enum GlobError {
    /// Include glob pattern could not be parsed.
    #[snafu(display("Include glob pattern {} could not be parsed: {}", glob, error))]
    InvalidIncludePattern {
        /// The glob string that produced the error.
        glob: String,
        /// The underlying error.
        error: globwalk::GlobError,
    },
    /// Exclude glob pattern could not be parsed.
    #[snafu(display("Exclude glob pattern {} could not be parsed: {}", glob, error))]
    InvalidExcludePattern {
        /// The glob string that produced the error.
        glob: String,
        /// The underlying error.
        error: glob::PatternError,
    },
    /// Failed while iterating on the glob.
    #[snafu(display("Failed while iterating on the glob {}: {}", glob, error))]
    WalkError {
        /// The glob string that produced the error.
        glob: String,
        /// The underlying error.
        error: globwalk::WalkError,
    },
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
            .map(|include_pattern| -> Result<_, _> {
                let glob =
                    glob(include_pattern).map_err(|error| GlobError::InvalidIncludePattern {
                        glob: include_pattern.to_owned(),
                        error,
                    })?;

                Ok(glob)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let include_patterns = include_patterns.to_owned();

        let exclude_patterns = exclude_patterns
            .iter()
            .map(|exclude_pattern| -> Result<_, _> {
                let pattern = Pattern::new(exclude_pattern).map_err(|error| {
                    GlobError::InvalidExcludePattern {
                        glob: exclude_pattern.to_owned(),
                        error,
                    }
                })?;

                Ok(pattern)
            })
            .collect::<Result<Vec<_>, _>>()?;

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
            let glob = glob(include_pattern).map_err(|error| GlobError::InvalidIncludePattern {
                glob: include_pattern.to_owned(),
                error,
            })?;

            for dir_entry in glob {
                let path = dir_entry
                    .map_err(|error| GlobError::WalkError {
                        glob: include_pattern.to_owned(),
                        error,
                    })?
                    .into_path();

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
