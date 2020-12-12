//! A state implementation backed by [`evmap`].

use crate::kubernetes::{
    debounce::Debounce,
    hash_value::{self, HashValue},
};
use async_trait::async_trait;
use evmap::WriteHandle;
use futures::future::BoxFuture;
use std::hash::Hash;
use std::time::Duration;

/// The type that extracts a key from the value.
pub trait Indexer<T>: Send {
    /// The type of the key to extract from the value.
    type IndexValueType: Eq + Hash + Clone + Send;

    /// Index the value
    fn index(&self, resource: &T) -> Option<Self::IndexValueType>;
}

/// A [`WriteHandle`] wrapper that implements [`super::Write`].
/// For use as a state writer implementation for
/// [`crate::kubernetes::Reflector`].
pub struct Writer<T, I>
where
    T: hash_value::Identity + Send,
    <T as hash_value::Identity>::IdentityType: ToOwned,
    <<T as hash_value::Identity>::IdentityType as ToOwned>::Owned: Hash + Eq + Clone + Send,
    I: Indexer<T> + Send,
{
    inner: WriteHandle<<I as Indexer<T>>::IndexValueType, Value<T>>,
    indexer: I,
    debounced_flush: Option<Debounce>,
}

impl<T, I> Writer<T, I>
where
    T: hash_value::Identity + Send,
    <T as hash_value::Identity>::IdentityType: ToOwned,
    <<T as hash_value::Identity>::IdentityType as ToOwned>::Owned: Hash + Eq + Clone + Send,
    I: Indexer<T> + Send,
{
    /// Take a [`WriteHandle`], initialize it and return it wrapped with
    /// [`Self`].
    pub fn new(
        mut inner: WriteHandle<<I as Indexer<T>>::IndexValueType, Value<T>>,
        indexer: I,
        flush_debounce_timeout: Option<Duration>,
    ) -> Self {
        // Prepare inner.
        inner.purge();
        inner.refresh();

        // Prepare flush debounce.
        let debounced_flush = flush_debounce_timeout.map(Debounce::new);

        Self {
            inner,
            indexer,
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
impl<T, I> super::Write for Writer<T, I>
where
    T: hash_value::Identity + Send,
    <T as hash_value::Identity>::IdentityType: ToOwned,
    <<T as hash_value::Identity>::IdentityType as ToOwned>::Owned: Hash + Eq + Clone + Send,
    I: Indexer<T> + Send,
{
    type Item = T;

    async fn add(&mut self, item: Self::Item) {
        if let Some((key, value)) = kv(&self.indexer, item) {
            self.inner.insert(key, value);
            self.debounced_flush();
        }
    }

    async fn update(&mut self, item: Self::Item) {
        if let Some((key, value)) = kv(&self.indexer, item) {
            self.inner.update(key, value);
            self.debounced_flush();
        }
    }

    async fn delete(&mut self, item: Self::Item) {
        if let Some((key, _value)) = kv(&self.indexer, item) {
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
impl<T, I> super::MaintainedWrite for Writer<T, I>
where
    T: hash_value::Identity + Send,
    <T as hash_value::Identity>::IdentityType: ToOwned,
    <<T as hash_value::Identity>::IdentityType as ToOwned>::Owned: Hash + Eq + Clone + Send,
    I: Indexer<T> + Send,
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

/// Build a key/value pair for using in [`evmap`] from an indexer.
fn kv<T, I>(indexer: &I, resource: T) -> Option<(<I as Indexer<T>>::IndexValueType, Value<T>)>
where
    T: hash_value::Identity + Send,
    <T as hash_value::Identity>::IdentityType: ToOwned,
    <<T as hash_value::Identity>::IdentityType as ToOwned>::Owned: Hash + Eq + Clone + Send,
    I: Indexer<T> + Send,
{
    let key = indexer.index(&resource)?;
    let value = Box::new(HashValue::new(resource));
    Some((key, value))
}

/// A simple indexer that delegates the indexing to the identity.
///
/// That is it will index the resources by whatever their identity is,
/// which will be the `uid` in the general case.
pub struct IdentityIndexer;

impl<T> Indexer<T> for IdentityIndexer
where
    T: hash_value::Identity + Send,
    <T as hash_value::Identity>::IdentityType: ToOwned,
    <<T as hash_value::Identity>::IdentityType as ToOwned>::Owned: Hash + Eq + Clone + Send,
{
    type IndexValueType = <<T as hash_value::Identity>::IdentityType as ToOwned>::Owned;

    fn index(&self, resource: &T) -> Option<Self::IndexValueType> {
        resource.identity().map(ToOwned::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::kubernetes::state::{MaintainedWrite, Write};
    use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};

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
        let (key, val) = kv(&IdentityIndexer, pod.clone()).unwrap();
        assert_eq!(key, "uid");
        assert_eq!(val, Box::new(HashValue::new(pod)));
    }

    #[tokio::test]
    async fn test_without_debounce() {
        let (state_reader, state_writer) = evmap::new();
        let mut state_writer = Writer::new(state_writer, IdentityIndexer, None);

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
        let mut state_writer =
            Writer::new(state_writer, IdentityIndexer, Some(flush_debounce_timeout));

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

    #[tokio::test]
    async fn test_operation_semantics_identity_indexer_clashing_add() {
        let (state_reader, state_writer) = evmap::new();
        let mut state_writer = Writer::new(state_writer, IdentityIndexer, None);

        state_writer.add(make_pod("uid0")).await;
        state_writer.add(make_pod("uid0")).await;

        assert_eq!(state_reader.len(), 1);

        assert_eq!(state_reader.get("uid0").unwrap().len(), 1);

        drop(state_writer);
    }

    #[tokio::test]
    async fn test_custom_indexer() {
        struct CustomIndexer;

        // Index the `Pod`s using an arbitrary label.
        impl Indexer<Pod> for CustomIndexer {
            type IndexValueType = String;
            fn index(&self, resource: &Pod) -> Option<Self::IndexValueType> {
                resource.metadata.name.clone()
            }
        }

        fn make_custom_pod(uid: &str, name: &str) -> Pod {
            Pod {
                metadata: ObjectMeta {
                    uid: Some(uid.to_owned()),
                    name: Some(name.to_owned()),
                    ..ObjectMeta::default()
                },
                ..Pod::default()
            }
        }

        let (state_reader, state_writer) = evmap::new();
        let mut state_writer = Writer::new(state_writer, CustomIndexer, None);

        // Insert `Pod`s with a name collision.
        state_writer.add(make_custom_pod("uid0", "name0")).await;
        state_writer.add(make_custom_pod("uid1", "name0")).await;

        // Assert that the keys look as expected.
        assert_eq!(state_reader.len(), 1); // we only have one key, so len should be 1!

        {
            let pods = state_reader
                .get("name0")
                .expect("name0 has to have a value");
            assert_eq!(pods.len(), 2); // we've inserted two `Pod`s with colliding index values.

            let mut expected_uids: HashSet<_> = vec!["uid0", "uid1"].into_iter().collect();
            for pod in pods.iter() {
                let uid = pod.metadata.uid.expect("all test pods should've had uids");
                assert!(
                    expected_uids.remove(uid),
                    "unexpected or already seed uid: {}",
                    uid
                );
            }
            assert!(expected_uids.is_empty());
        }

        drop(state_writer);
    }
}
