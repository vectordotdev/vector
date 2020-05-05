//! Abstractions to allow configuring ways to provide the paths list for the
//! file source to watch and read.

#![deny(missing_docs)]

use std::path::PathBuf;

pub mod glob;

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
/// See: https://github.com/rust-lang/rust/issues/44265
///
/// We use a [`Vec`] here as a workaround.
///
/// The performance penalty is negligible, since [`crate::FileServer`] polls
/// for paths only every minute. The expected amount of yelded paths is small,
/// and there's plenty of time to east up the extra allocations.
/// Of course, it would be better to avoid putting the paths in memory all at
/// once - so improvements are welcome.
pub trait PathsProvider {
    /// Provides a set of paths.
    fn paths(&self) -> Vec<PathBuf>;
}
