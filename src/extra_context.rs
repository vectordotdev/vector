//! ExtraContext is used for passing extra data to Vector's components when Vector is used as a library.
use std::{
    marker::{Send, Sync},
    sync::Arc,
};

use anymap::{
    any::{Any, IntoBox},
    Map,
};

/// Structure containing any extra data.
/// The data is held in an [`Arc`] so is cheap to clone.
#[derive(Clone)]
pub struct ExtraContext {
    context: Arc<Map<dyn Any + Send + Sync>>,
}

impl Default for ExtraContext {
    fn default() -> Self {
        Self {
            context: Arc::new(Map::new()),
        }
    }
}

impl ExtraContext {
    /// Create a new `ExtraContext` with the provided [`anymap::Map`].
    pub fn new(context: Map<dyn Any + Send + Sync>) -> Self {
        Self {
            context: Arc::new(context),
        }
    }

    /// Get an object from the context.
    pub fn get<T>(&self) -> Option<&T>
    where
        T: IntoBox<dyn Any + Send + Sync>,
    {
        self.context.get()
    }
}
