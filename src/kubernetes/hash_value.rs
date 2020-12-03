//! A wrapper to implement hash for k8s resource objects.

use k8s_openapi::{apimachinery::pkg::apis::meta::v1::ObjectMeta, Metadata};
use std::hash::{Hash, Hasher};
use std::ops::Deref;

/// Provide an identity for a value - (possibly) another value of an
/// assosicated type that uniquely identifies the value of the type
/// that implements `Identity` trait.
///
/// This type is used for the purposes of extracting the hashable values from
/// non-hashable types for the [`HashValue`].
pub trait Identity {
    /// The type of the identity.
    type IdentityType: ?Sized + PartialEq + Eq + Hash;

    /// Return an identity value for `self`.
    fn identity(&self) -> Option<&'_ Self::IdentityType>;
}

/// A wrapper that provides a [`Hash`] implementation for any k8s resource
/// object.
/// Delegates to object uid for hashing and equality.
#[derive(Debug)]
pub struct HashValue<T: Identity>(T);

impl<T> HashValue<T>
where
    T: Identity,
{
    /// Create a new [`HashValue`] by wrapping a value of `T`.
    pub fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T> PartialEq<Self> for HashValue<T>
where
    T: Identity,
{
    fn eq(&self, other: &Self) -> bool {
        match (self.identity(), other.identity()) {
            (Some(a), Some(b)) => a.eq(b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl<T> Eq for HashValue<T> where T: Identity {}

impl<T> Hash for HashValue<T>
where
    T: Identity,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identity().hash(state)
    }
}

impl<T> Deref for HashValue<T>
where
    T: Identity,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> AsRef<T> for HashValue<T>
where
    T: Identity,
{
    fn as_ref(&self) -> &T {
        &self.0
    }
}

/// The identity for the types implementing `Metadata<Ty = ObjectMeta>` is
/// their `uid`.
impl<T> Identity for T
where
    T: Metadata<Ty = ObjectMeta>,
{
    type IdentityType = str;

    fn identity(&self) -> Option<&'_ Self::IdentityType> {
        let ObjectMeta { ref uid, .. } = self.metadata();
        uid.as_deref()
    }
}
