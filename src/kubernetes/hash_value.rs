//! A wrapper to implement hash for k8s resource objects.

use k8s_openapi::{apimachinery::pkg::apis::meta::v1::ObjectMeta, Metadata};
use std::hash::{Hash, Hasher};
use std::ops::Deref;

/// A wrapper that provdies a [`Hash`] implementation for any k8s resource
/// object.
/// Delegates to object uid for hashing and equality.
#[derive(Debug)]
pub struct HashValue<T: Metadata<Ty = ObjectMeta>>(T);

impl<T> HashValue<T>
where
    T: Metadata<Ty = ObjectMeta>,
{
    /// Create a new [`HashValue`] by wrapping a value of `T`.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Get the `uid` from the `T`'s [`Metadata`] (if any).
    pub fn uid(&self) -> Option<&str> {
        let ObjectMeta { ref uid, .. } = self.0.metadata();
        let uid = uid.as_ref()?;
        Some(uid.as_str())
    }
}

impl<T> PartialEq<Self> for HashValue<T>
where
    T: Metadata<Ty = ObjectMeta>,
{
    fn eq(&self, other: &Self) -> bool {
        match (self.uid(), other.uid()) {
            (Some(a), Some(b)) => a.eq(b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl<T> Eq for HashValue<T> where T: Metadata<Ty = ObjectMeta> {}

impl<T> Hash for HashValue<T>
where
    T: Metadata<Ty = ObjectMeta>,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uid().hash(state)
    }
}

impl<T> Deref for HashValue<T>
where
    T: Metadata<Ty = ObjectMeta>,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> AsRef<T> for HashValue<T>
where
    T: Metadata<Ty = ObjectMeta>,
{
    fn as_ref(&self) -> &T {
        &self.0
    }
}
