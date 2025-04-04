//! Abstractions to allow configuring ways to provide the paths list for the
//! file source to watch and read.

#![deny(missing_docs)]

use std::path::PathBuf;

pub mod glob;

/// Provides a notify-based implementation of the `PathsProvider` trait.
///
/// This implementation uses filesystem notifications to discover files that match
/// include patterns and don't match exclude patterns, instead of continuously globbing.
pub mod notify;

/// Provides a boxed implementation of the `PathsProvider` trait.
///
/// This allows us to use dynamic dispatch with PathsProvider implementations.
pub mod boxed;

/// Represents the ability to enumerate paths.
///
/// For use at [`crate::FileServer`].
///
/// # Notes
///
/// Ideally we'd use an iterator with bound lifetime here:
///
/// ```ignore
/// type Iter<'a>: Iterator<Item = PathBuf> + 'a;
/// fn paths(&self) -> Self::Iter<'_>;
/// ```
///
/// However, that's currently unavailable at Rust.
/// See: <https://github.com/rust-lang/rust/issues/44265>
///
/// We use an `IntoIter` here as a workaround.
pub trait PathsProvider {
    /// Provides the iterator that returns paths.
    type IntoIter: IntoIterator<Item = PathBuf>;

    /// Provides a set of paths.
    fn paths(&self) -> Self::IntoIter;
}
