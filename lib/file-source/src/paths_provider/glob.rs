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

    /// Iterates over the paths.
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            include_patterns_iter: self.include_patterns.iter(),
            exclude_patterns: self.exclude_patterns.as_slice(),
            glob_match_options: &self.glob_match_options,
            current_glob_iter: None,
        }
    }
}

/// Iterator for [`Glob`].
pub struct Iter<'a> {
    include_patterns_iter: std::slice::Iter<'a, String>,
    exclude_patterns: &'a [Pattern],
    glob_match_options: &'a glob::MatchOptions,
    current_glob_iter: Option<glob::Paths>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            current_glob_iter,
            exclude_patterns,
            glob_match_options,
            include_patterns_iter,
        } = self;

        let exclude_predicate = |candidate_path: &PathBuf| -> bool {
            exclude_patterns.iter().any(|exclude_pattern| {
                let candidate_path_str = candidate_path.to_str().unwrap();
                exclude_pattern.matches(candidate_path_str)
            })
        };

        loop {
            // See if we have an iterator it progress.
            {
                if let Some(ref mut glob_iter) = current_glob_iter {
                    match glob_iter.next() {
                        // If we exaust the current glob iter we need to try to
                        // update it. To do it we just allow execution to continue
                        // beyond this `if let`.
                        None => {}
                        // If we got the path, and it's not excluded - return
                        // it!
                        Some(Ok(path)) if !exclude_predicate(&path) => return Some(path),
                        // Everything else we just mangle and continue the loop.
                        Some(_) => continue,
                    }
                }
            }

            // We only get here if we need to update the `current_glob_iter`.

            // Try fetching a new pattern. If the patterns iterator is
            // exausted - we're done.
            let pattern = include_patterns_iter.next()?;

            // Glob over the new pattern. Panic we we encounter any
            // issue.
            let next_iter = glob::glob_with(pattern.as_str(), glob_match_options)
                .expect("failed to read glob pattern");

            // We got the new glob iter, stash it and continiue the loop.
            *current_glob_iter = Some(next_iter);
        }
    }
}

impl<'a> PathsProvider for &'a Glob {
    type IntoIter = Iter<'a>;

    fn paths(&self) -> Self::IntoIter {
        self.iter()
    }
}

/// Workaround for the absense of the GATs in Rust.
pub mod gat_workaround {
    use super::*;
    use std::sync::Arc;

    impl Glob {
        /// Create a new [`Glob`] wrapped in [`Arc`].
        ///
        /// `Arc<Glob>` implements [`PathsProvider`] with low overhead.
        /// This complexity is required while Rust doesn't have GATs.
        ///
        /// Returns `None` if patterns aren't valid.
        pub fn new_arc(
            include_patterns: &[PathBuf],
            exclude_patterns: &[PathBuf],
            glob_match_options: MatchOptions,
        ) -> Option<Arc<Self>> {
            Some(Arc::new(Self::new(
                include_patterns,
                exclude_patterns,
                glob_match_options,
            )?))
        }
    }

    rental! {
        #[allow(missing_docs)]
        pub mod rent_iter {
            use super::*;
            #[rental]
            pub struct RentIter {
                head: Arc<Glob>,
                tail: Iter<'head>,
            }
        }
    }

    impl Iterator for rent_iter::RentIter {
        type Item = PathBuf;

        fn next(&mut self) -> Option<Self::Item> {
            self.rent_mut(|val| Iter::next(val))
        }
    }

    impl PathsProvider for Arc<Glob> {
        type IntoIter = rent_iter::RentIter;

        fn paths(&self) -> Self::IntoIter {
            rent_iter::RentIter::new(self.clone(), |glob| glob.iter())
        }
    }
}
