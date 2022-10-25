use std::fmt;

use vector_config::configurable_component;
pub use vector_core::config::ComponentKey;

/// A list of upstream [source][sources] or [transform][transforms] IDs.
///
/// Wildcards (`*`) are supported.
///
/// See [configuration][configuration] for more info.
///
/// [sources]: https://vector.dev/docs/reference/configuration/sources/
/// [transforms]: https://vector.dev/docs/reference/configuration/transforms/
/// [configuration]: https://vector.dev/docs/reference/configuration/
#[configurable_component]
#[configurable(metadata(
    docs::examples = "my-source-or-transform-id",
    docs::examples = "prefix-*"
))]
#[derive(Clone, Debug)]
pub struct Inputs<T>(#[configurable(transparent)] Vec<T>);

impl<T> Default for Inputs<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> std::ops::Deref for Inputs<T> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        self.0.as_slice()
    }
}

impl<T> std::ops::DerefMut for Inputs<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        self.0.as_mut_slice()
    }
}

impl<T, U> PartialEq<&[U]> for Inputs<T>
where
    T: PartialEq<U>,
{
    fn eq(&self, other: &&[U]) -> bool {
        self.0.as_slice() == &other[..]
    }
}

impl<T, U> PartialEq<Vec<U>> for Inputs<T>
where
    T: PartialEq<U>,
{
    fn eq(&self, other: &Vec<U>) -> bool {
        &self.0 == other
    }
}

impl<T> Extend<T> for Inputs<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.0.extend(iter)
    }
}

impl<T> IntoIterator for Inputs<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a Inputs<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T> FromIterator<T> for Inputs<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(Vec::from_iter(iter.into_iter()))
    }
}

/// Component output identifier.
#[configurable_component]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OutputId {
    /// The component to which the output belongs.
    pub component: ComponentKey,

    /// The output port name, if not the default.
    pub port: Option<String>,
}

impl fmt::Display for OutputId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.port {
            None => self.component.fmt(f),
            Some(port) => write!(f, "{}.{}", self.component, port),
        }
    }
}

impl From<ComponentKey> for OutputId {
    fn from(key: ComponentKey) -> Self {
        Self {
            component: key,
            port: None,
        }
    }
}

impl From<&ComponentKey> for OutputId {
    fn from(key: &ComponentKey) -> Self {
        Self::from(key.clone())
    }
}

impl From<(&ComponentKey, String)> for OutputId {
    fn from((key, name): (&ComponentKey, String)) -> Self {
        Self {
            component: key.clone(),
            port: Some(name),
        }
    }
}

// This panicking implementation is convenient for testing, but should never be enabled for use
// outside of tests.
#[cfg(test)]
impl From<&str> for OutputId {
    fn from(s: &str) -> Self {
        assert!(
            !s.contains('.'),
            "Cannot convert dotted paths to strings without more context"
        );
        let component = ComponentKey::from(s);
        component.into()
    }
}
