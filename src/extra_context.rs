//! ExtraContext is used for passing extra data to Vector's components when Vector is used as a library.
use std::{
    marker::{Send, Sync},
    sync::Arc,
};

use anymap::{
    core::any::{Any, IntoBox},
    Map,
};

/// Structure containing any extra data.
/// The data is held in an [`Arc`] so is cheap to clone.
#[derive(Clone)]
pub struct ExtraContext(Arc<Map<dyn Any + Send + Sync>>);

impl Default for ExtraContext {
    fn default() -> Self {
        Self(Arc::new(Map::new()))
    }
}

impl ExtraContext {
    /// Create a new `ExtraContext` with the provided [`anymap::Map`].
    pub fn new(context: Map<dyn Any + Send + Sync>) -> Self {
        Self(Arc::new(context))
    }

    /// Create a new `ExtraContext` that contains the single passed in value.
    pub fn single_value<T: Any + Send + Sync>(value: T) -> Self {
        let mut map = Map::new();
        map.insert(value);
        Self(Arc::new(map))
    }

    /// Get an object from the context.
    pub fn get<T>(&self) -> Option<&T>
    where
        T: IntoBox<dyn Any + Send + Sync>,
    {
        self.0.get()
    }

    /// Get an object from the context, if it doesn't exist return the default.
    pub fn get_or_default<T>(&self) -> T
    where
        T: IntoBox<dyn Any + Send + Sync> + Clone + Default,
    {
        self.0.get().cloned().unwrap_or_default()
    }
}
