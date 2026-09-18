//! Abstractions to allow configuring ways to provide the paths list for the
//! file source to watch and read.

#![deny(missing_docs)]

use std::path::PathBuf;

pub use glob::MatchOptions as GlobMatchOptions;

/// Provides a notify-based implementation of the `PathsProvider` trait.
///
/// This implementation uses filesystem notifications to discover files that match
/// include patterns and don't match exclude patterns, instead of continuously globbing.
pub mod notify;

use std::future::Future;

/// Represents the ability to enumerate paths.
///
/// For use at [`crate::FileServer`].
///
/// # Notes
///
/// This trait uses async methods to allow for more efficient file discovery.
pub trait PathsProvider {
    /// Provides the iterator that returns paths.
    type IntoIter: IntoIterator<Item = PathBuf>;

    /// Wait until filesystem activity may require another read or discovery pass.
    fn wait_for_changes(&mut self) -> impl Future<Output = ()> + Send;

    /// Provides a set of paths asynchronously.
    fn paths(&mut self, should_glob: bool) -> impl Future<Output = Self::IntoIter> + Send;
}
