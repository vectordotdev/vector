//! A state implementation backed by [`evmap10`].

use crate::kubernetes::hash_value::HashValue;
use evmap10::WriteHandle;
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::ObjectMeta, Metadata};

/// A [`WriteHandle`] wrapper that implements [`super::Write`].
/// For use as a state writer implementation for
/// [`crate::kubernetes::Reflector`].
pub struct Writer<T>
where
    T: Metadata<Ty = ObjectMeta>,
{
    inner: WriteHandle<String, Value<T>>,
}

impl<T> Writer<T>
where
    T: Metadata<Ty = ObjectMeta>,
{
    /// Take a [`WriteHandle`], initialize it and return it wrapped with
    /// [`Self`].
    pub fn new(mut inner: WriteHandle<String, Value<T>>) -> Self {
        inner.purge();
        inner.refresh();
        Self { inner }
    }
}

impl<T> super::Write for Writer<T>
where
    T: Metadata<Ty = ObjectMeta>,
{
    type Item = T;

    // TODO: debounce `flush` so that when a bunch of events arrive in a row
    // within a certain small time window we commit all of them at once.
    // This will improve the state behaivor at resync.

    fn add(&mut self, item: Self::Item) {
        if let Some((key, value)) = kv(item) {
            self.inner.insert(key, value);
            self.inner.flush();
        }
    }

    fn update(&mut self, item: Self::Item) {
        if let Some((key, value)) = kv(item) {
            self.inner.update(key, value);
            self.inner.flush();
        }
    }

    fn delete(&mut self, item: Self::Item) {
        if let Some((key, _value)) = kv(item) {
            self.inner.empty(key);
            self.inner.flush();
        }
    }

    fn resync(&mut self) {
        self.inner.purge();
        // We do not flush on resync until the next per-item operation
        // arrives.
        // This way we preserve the old state while waiting for the data to
        // populate the new state.
    }
}

/// An alias to the value used at [`evmap`].
pub type Value<T> = Box<HashValue<T>>;

/// Build a key value pair for using in [`evmap`].
fn kv<T: Metadata<Ty = ObjectMeta>>(object: T) -> Option<(String, Value<T>)> {
    let value = Box::new(HashValue::new(object));
    let key = value.uid()?.to_owned();
    Some((key, value))
}
