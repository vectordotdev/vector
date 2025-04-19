use std::path::PathBuf;

use super::PathsProvider;

/// A wrapper around a boxed PathsProvider implementation.
///
/// This allows us to use dynamic dispatch with PathsProvider implementations.
pub struct BoxedPathsProvider {
    inner: Box<dyn PathsProvider<IntoIter = Vec<PathBuf>> + Send>,
}

impl BoxedPathsProvider {
    /// Create a new BoxedPathsProvider
    pub fn new<P: PathsProvider<IntoIter = Vec<PathBuf>> + Send + 'static>(provider: P) -> Self {
        BoxedPathsProvider {
            inner: Box::new(provider),
        }
    }
}

impl PathsProvider for BoxedPathsProvider {
    type IntoIter = Vec<PathBuf>;

    fn paths(&self) -> Self::IntoIter {
        self.inner.paths()
    }
}
