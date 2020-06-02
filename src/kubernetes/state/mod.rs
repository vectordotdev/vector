//! Local representation of the Kubernetes API resources state.

use k8s_openapi::{apimachinery::pkg::apis::meta::v1::ObjectMeta, Metadata};

pub mod evmap;

/// Provides the interface for write access to the cached state.
/// Used by [`super::reflector::Reflector`].
///
/// This abstraction allows easily stacking storage behaviour logic, without
/// exploding the complexity at the [`super::reflector::Reflector`] level.
pub trait Write {
    /// A type of the k8s resource the state operates on.
    type Item: Metadata<Ty = ObjectMeta>;

    /// Add an object to the state.
    fn add(&mut self, item: Self::Item);

    /// Update an object at the state.
    fn update(&mut self, item: Self::Item);

    /// Delete on object from the state.
    fn delete(&mut self, item: Self::Item);

    /// Notify the state that resync is in progress.
    fn resync(&mut self);
}
