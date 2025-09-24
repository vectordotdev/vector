use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use super::PathsProvider;

/// A wrapper around a boxed PathsProvider implementation.
///
/// This allows us to use dynamic dispatch with PathsProvider implementations.
pub struct BoxedPathsProvider {
    inner: Box<dyn PathsProvider<IntoIter = Vec<PathBuf>> + Send + Sync + 'static>,
}

impl BoxedPathsProvider {
    /// Create a new BoxedPathsProvider
    pub fn new<P>(provider: P) -> Self
    where
        P: PathsProvider<IntoIter = Vec<PathBuf>> + Send + Sync + 'static,
    {
        BoxedPathsProvider {
            inner: Box::new(provider),
        }
    }
}

impl PathsProvider for BoxedPathsProvider {
    type IntoIter = Vec<PathBuf>;

    fn paths(
        &self,
        should_glob: bool,
    ) -> Pin<Box<dyn Future<Output = Self::IntoIter> + Send + '_>> {
        // Return the inner future directly
        self.inner.paths(should_glob)
    }
}
