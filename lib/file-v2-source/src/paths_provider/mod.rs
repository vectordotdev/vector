//! Abstractions to allow configuring ways to provide the paths list for the
//! file source to watch and read.

#![deny(missing_docs)]

use std::{collections::HashSet, path::PathBuf};

pub use glob::MatchOptions as GlobMatchOptions;

/// Provides a notify-based implementation of the `PathsProvider` trait.
///
/// This implementation uses filesystem notifications to discover files that match
/// include patterns and don't match exclude patterns, instead of continuously globbing.
pub mod notify;

use std::future::Future;

/// Paths whose identities need to be checked.
#[derive(Debug, PartialEq, Eq)]
pub enum PathUpdates {
    /// A complete snapshot. Previously watched paths not present may have disappeared.
    Snapshot(HashSet<PathBuf>),
    /// Only paths affected by notifications.
    Changed {
        /// Created or modified paths whose fingerprints need checking.
        updated: HashSet<PathBuf>,
        /// Removed paths whose existing readers may still have data to drain.
        removed: HashSet<PathBuf>,
    },
}

impl PathUpdates {
    pub(crate) fn into_paths(self) -> HashSet<PathBuf> {
        match self {
            Self::Snapshot(paths) | Self::Changed { updated: paths, .. } => paths,
        }
    }
}

/// Represents the ability to enumerate paths.
///
/// For use at [`crate::FileServer`].
///
/// # Notes
///
/// This trait uses async methods to allow for more efficient file discovery.
pub trait PathsProvider {
    /// Wait until filesystem activity may require another read or discovery pass.
    fn wait_for_changes(&mut self) -> impl Future<Output = ()> + Send;

    /// Return changed paths, or a full snapshot when reconciliation is requested
    /// or notifications indicate that changes may have been missed.
    fn paths(&mut self, should_glob: bool) -> impl Future<Output = PathUpdates> + Send;
}
