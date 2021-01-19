//! A state implementation backed by [`evmap`].

use crate::kubernetes::{debounce::Debounce, hash_value::HashValue};
use async_trait::async_trait;
use evmap::WriteHandle;
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
    /// This is intended to improve the state behavior at re-sync - by delaying
    /// the `flush` propagation, we maximize the time `evmap` remains populated,
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
        // By omitting the flush here, we cache the results from the
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kubernetes::state::{MaintainedWrite, Write};
    use k8s_openapi::api::core::v1::Pod;

    fn make_pod(uid: &str) -> Pod {
        Pod {
            metadata: ObjectMeta {
                uid: Some(uid.to_owned()),
                ..ObjectMeta::default()
            },
            ..Pod::default()
        }
    }

    #[test]
    fn test_kv() {
        let pod = make_pod("uid");
        let (key, val) = kv(pod.clone()).unwrap();
        assert_eq!(key, "uid");
        assert_eq!(val, Box::new(HashValue::new(pod)));
    }

    #[test]
    fn test_kv_static_pod() {
        let pod = Pod {
            metadata: ObjectMeta {
                uid: Some("uid".to_owned()),
                annotations: Some(
                    vec![(
                        "kubernetes.io/config.mirror".to_owned(),
                        "config-hashsum".to_owned(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..ObjectMeta::default()
            },
            ..Pod::default()
        };
        let (key, val) = kv(pod.clone()).unwrap();
        assert_eq!(key, "config-hashsum");
        assert_eq!(val, Box::new(HashValue::new(pod)));
    }

    #[tokio::test]
    async fn test_without_debounce() {
        let (state_reader, state_writer) = evmap::new();
        let mut state_writer = Writer::new(state_writer, None);

        assert_eq!(state_reader.is_empty(), true);
        assert!(state_writer.maintenance_request().is_none());

        state_writer.add(make_pod("uid0")).await;

        assert_eq!(state_reader.is_empty(), false);
        assert!(state_writer.maintenance_request().is_none());

        drop(state_writer);
    }

    #[tokio::test]
    async fn test_with_debounce() {
        // Due to https://github.com/tokio-rs/tokio/issues/2090 we're not
        // pausing the time.

        let (state_reader, state_writer) = evmap::new();
        let flush_debounce_timeout = Duration::from_millis(100);
        let mut state_writer = Writer::new(state_writer, Some(flush_debounce_timeout));

        assert_eq!(state_reader.is_empty(), true);
        assert!(state_writer.maintenance_request().is_none());

        state_writer.add(make_pod("uid0")).await;
        state_writer.add(make_pod("uid1")).await;

        assert_eq!(state_reader.is_empty(), true);
        assert!(state_writer.maintenance_request().is_some());

        let join = tokio::spawn(async move {
            let mut state_writer = state_writer;
            state_writer.maintenance_request().unwrap().await;
            state_writer.perform_maintenance().await;
            state_writer
        });

        assert_eq!(state_reader.is_empty(), true);

        tokio::time::delay_for(flush_debounce_timeout * 2).await;
        let mut state_writer = join.await.unwrap();

        assert_eq!(state_reader.is_empty(), false);
        assert!(state_writer.maintenance_request().is_none());

        drop(state_writer);
    }
}
