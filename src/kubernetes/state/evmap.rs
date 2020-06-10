//! A state implementation backed by [`evmap10`].

use crate::kubernetes::{debounce::Debounce, hash_value::HashValue};
use async_trait::async_trait;
use evmap10::WriteHandle;
use futures::future::BoxFuture;
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::ObjectMeta, Metadata};
use std::time::Duration;

/// A [`WriteHandle`] wrapper that implements [`super::Write`].
/// For use as a state writer implementation for
/// [`crate::kubernetes::Reflector`].
pub struct Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    inner: WriteHandle<String, Value<T>>,
    debounced_flush: Option<Debounce>,
}

impl<T> Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    /// Take a [`WriteHandle`], initialize it and return it wrapped with
    /// [`Self`].
    pub fn new(
        mut inner: WriteHandle<String, Value<T>>,
        flush_debounce_timeout: Option<Duration>,
    ) -> Self {
        // Prepare inner.
        inner.purge();
        inner.refresh();

        // Prepare flush debounce.
        let debounced_flush = flush_debounce_timeout.map(Debounce::new);

        Self {
            inner,
            debounced_flush,
        }
    }

    /// Debounced `flush`.
    /// When a number of flush events arrive un a row, we buffer them such that
    /// only the last one in the chain is propagated.
    /// This is intended to improve the state behaivor at resync - by delaying
    /// the `flush` proparagion, we maximize the time `evmap` remains populated,
    /// ideally allowing a single transition from non-populated to populated
    /// state.
    fn debounced_flush(&mut self) {
        if let Some(ref mut debounced_flush) = self.debounced_flush {
            debounced_flush.signal();
        } else {
            self.inner.flush();
        }
    }
}

#[async_trait]
impl<T> super::Write for Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    type Item = T;

    async fn add(&mut self, item: Self::Item) {
        if let Some((key, value)) = kv(item) {
            self.inner.insert(key, value);
            self.debounced_flush();
        }
    }

    async fn update(&mut self, item: Self::Item) {
        if let Some((key, value)) = kv(item) {
            self.inner.update(key, value);
            self.debounced_flush();
        }
    }

    async fn delete(&mut self, item: Self::Item) {
        if let Some((key, _value)) = kv(item) {
            self.inner.empty(key);
            self.debounced_flush();
        }
    }

    async fn resync(&mut self) {
        // By omiting the flush here, we cache the results from the
        // previous run until flush is issued when the new events
        // begin arriving, reducing the time during which the state
        // has no data.
        self.inner.purge();
    }
}

#[async_trait]
impl<T> super::MaintainedWrite for Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    fn maintenance_request(&mut self) -> Option<BoxFuture<'_, ()>> {
        if let Some(ref mut debounced_flush) = self.debounced_flush {
            if debounced_flush.is_debouncing() {
                return Some(Box::pin(debounced_flush.debounced()));
            }
        }
        None
    }

    async fn perform_maintenance(&mut self) {
        if self.debounced_flush.is_some() {
            self.inner.flush();
        }
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
