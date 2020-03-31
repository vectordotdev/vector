//! Expiring Hash Map and related types. See [`ExpiringHashMap`].
#![warn(missing_docs)]

use futures::stream::StreamExt;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::time::{Duration, Instant};
use tokio::time::{delay_queue, DelayQueue, Error};

/// An expired item, holding the value and the key with an expiration
/// information.
pub type ExpiredItem<K, V> = (V, delay_queue::Expired<K>);

/// A [`HashMap`] that maintains deadlines for the keys via a [`DelayQueue`].
pub struct ExpiringHashMap<K, V> {
    map: HashMap<K, (V, delay_queue::Key)>,
    expiration_queue: DelayQueue<K>,
}

impl<K, V> Unpin for ExpiringHashMap<K, V> {}

impl<K, V> ExpiringHashMap<K, V>
where
    K: Eq + Hash + Clone,
{
    /// Create a new [`ExpiringHashMap`].
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            expiration_queue: DelayQueue::new(),
        }
    }

    /// Insert a new key with a TTL.
    pub fn insert(&mut self, key: K, value: V, ttl: Duration) {
        let delay_queue_key = self.expiration_queue.insert(key.clone(), ttl);
        self.map.insert(key, (value, delay_queue_key));
    }

    /// Insert a new value with a deadline.
    pub fn insert_at(&mut self, key: K, value: V, deadline: Instant) {
        let delay_queue_key = self
            .expiration_queue
            .insert_at(key.clone(), deadline.into());
        self.map.insert(key, (value, delay_queue_key));
    }

    /// Get a reference to the value by key.
    pub fn get<Q>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.get(k).map(|&(ref v, _)| v)
    }

    /// Get a mut reference to the value by key.
    pub fn get_mut<Q>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.get_mut(k).map(|&mut (ref mut v, _)| v)
    }

    /// Reset the deadline for a key, and return a mut ref to the value.
    pub fn reset_at<Q>(&mut self, k: &Q, when: Instant) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let (value, delay_queue_key) = self.map.get_mut(k)?;
        self.expiration_queue.reset_at(delay_queue_key, when.into());
        Some(value)
    }

    /// Reset the key if it exists, returning the value and the expiration
    /// information.
    pub fn remove<Q>(&mut self, k: &Q) -> Option<ExpiredItem<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let (value, expiration_queue_key) = self.map.remove(k)?;
        let expired = self.expiration_queue.remove(&expiration_queue_key);
        Some((value, expired))
    }

    /// Check whether the [`ExpiringHashMap`] is empty.
    /// If it's empty, the `next_expired` function immediately resolves to
    /// [`None`]. Be aware that this may cause a spinlock behaviour if the
    /// `next_expired` is polled in a loop while [`ExpiringHashMap`] is empty.
    /// See [`ExpiringHashMap::next_expired`] for more info.
    pub fn is_empty(&self) -> bool {
        self.expiration_queue.is_empty()
    }

    /// If the [`ExpiringHashMap`] is empty, immediately returns `None`.
    /// Otherwise, waits for the closest deadline, removes expired item and
    /// returns it.
    ///
    /// Be aware that misuse of this function may cause a spinlock! If you want
    /// to `select` on this future in a loop, be sure to check
    /// [`ExpiringHashMap::is_empty`] and skip polling on
    /// [`ExpiringHashMap::next_expired`] if the [`ExpiringHashMap`] is empty.
    /// Otherwise, when the [`ExpiringHashMap`] is empty you'll effectively get
    /// a spinlock on the first value insertion.
    ///
    /// We currently don't offer an API that would allow simply waiting for
    /// expired items regardless of what state the [`ExpiringHashMap`] is.
    /// This is a deliberate design decision, we went with it for the following
    /// reasons:
    /// 1. Use of `async fn`. One of the benefits of this API is that it relies
    ///    only on `async fn`s, and doesn't require manual `Future`
    ///    implementation. While this is not a problem in general, but there is
    ///    some value with doing it this way. With a switch to `async` across
    ///    our code base, the idea is that we should completely eliminate manual
    ///    `Future` implementations and poll fns. This is controversial, but we
    ///    decided to give it a try.
    /// 2. We don't know all the use cases, and exposing this kind of API might
    ///    make more sense, since it allows more flexibility.
    ///    We were choosing between, effectively, the current "drain"-like API,
    ///    and the "queue" like API.
    ///    Current ("drain"-like) API waits on the deadline or returns `None`
    ///    when there are no more items. Very similar how we [`Vec::drain`] iter
    ///    works.
    ///    The "queue"-like API would, pretty much, be simply waiting expired
    ///    items to appear. In the case of empty [`ExpiringHashMap`], we would
    ///    wait indefinitely - or until an item is inserted. This would be
    ///    possible to carry on, for instance, from a sibling branch of a
    ///    `select` statement, so the borrowing rules won't be a problem here.
    /// 3. We went over the following alternative signature:
    ///    ```ignore
    ///    pub fn next_expired(&mut self) -> Option<impl Future<Outcome = Result<ExpiredItem<K, V>, Error>>> {...}
    ///    ```
    ///    This captures the API restrictions a bit better, and should provide
    ///    less possibilities to misuse the API.
    ///    We didn't pick this one because it's not an `async fn` and we wanted
    ///    this, see (1) of this list. Furthermore, instead of doing a
    ///    `select { _ = map.next_expired(), if !map.is_empty() => { ... } }`
    ///    users would have to do
    ///    `let exp = map.next_expired(); select { _ = exp.unwrap(), if exp.is_some() => { ... } }`,
    ///    which is less readable and a bit harder to understand. Although it
    ///    has a possibility of a nicer generalization if `select` macro
    ///    supported a `Some(future)` kind of pattern matching, we decided to go
    ///    with other solution for now.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # let mut rt = tokio::runtime::Runtime::new().unwrap();
    /// # rt.block_on(async {
    /// use vector::expiring_hash_map::ExpiringHashMap;
    /// use std::time::Duration;
    ///
    /// let mut map: ExpiringHashMap<String, String> = ExpiringHashMap::new();
    ///
    /// loop {
    ///     tokio::select! {
    ///         // You need to ensure that this branch is disabled if the map
    ///         // is empty! Not doing this will result in a spinlock.
    ///         val = map.next_expired(), if !map.is_empty() => match val {
    ///             None => unreachable!(), // we never poll the empty map in the first place!
    ///             Some(Ok((val, _))) => {
    ///                 println!("Expired: {}", val);
    ///                 break;
    ///             }
    ///             Some(Err(err)) => panic!(format!("Timer error: {:?}", err)),
    ///         },
    ///         _ = tokio::time::delay_for(Duration::from_millis(100)) => map.insert(
    ///             "key".to_owned(),
    ///             "val".to_owned(),
    ///             Duration::from_millis(30),
    ///         ),
    ///     }
    /// }
    /// # });
    /// ```
    pub async fn next_expired(&mut self) -> Option<Result<ExpiredItem<K, V>, Error>> {
        let key = self.expiration_queue.next().await?;
        Some(key.map(|key| {
            let (value, _) = self.map.remove(key.get_ref()).unwrap();
            (value, key)
        }))
    }
}

impl<K, V> fmt::Debug for ExpiringHashMap<K, V>
where
    K: Eq + Hash + Clone,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExpiringHashMap").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::Poll;
    use tokio_test::{assert_pending, assert_ready, task};

    fn unwrap_ready<T>(poll: Poll<T>) -> T {
        assert_ready!(&poll);
        match poll {
            Poll::Ready(val) => val,
            _ => unreachable!(),
        }
    }

    #[test]
    fn next_expired_is_pending_with_empty_map() {
        let mut map = ExpiringHashMap::<String, String>::new();
        let mut fut = task::spawn(map.next_expired());
        assert!(unwrap_ready(fut.poll()).is_none());
    }

    #[tokio::test]
    async fn next_expired_is_pending_with_a_non_empty_map() {
        let mut map = ExpiringHashMap::<String, String>::new();

        map.insert("key".to_owned(), "val".to_owned(), Duration::from_secs(1));
        map.remove("key");

        let mut fut = task::spawn(map.next_expired());
        assert_pending!(fut.poll());
    }

    #[tokio::test]
    async fn next_expired_does_not_wake_when_the_value_is_available_upfront() {
        let mut map = ExpiringHashMap::<String, String>::new();

        let a_minute_ago = Instant::now() - Duration::from_secs(60);
        map.insert_at("key".to_owned(), "val".to_owned(), a_minute_ago);

        let mut fut = task::spawn(map.next_expired());
        assert_eq!(unwrap_ready(fut.poll()).unwrap().unwrap().0, "val");
        assert_eq!(fut.is_woken(), false);
    }

    // TODO: rewrite this test with tokio::time::clock when it's available.
    // For now we just wait for an actal second. We should just scroll time instead.
    // In theory, this is only possible when the runtime timer used in the
    // underlying delay queue and the means by which we fresse/adjust time are
    // working together.
    #[tokio::test]
    async fn next_expired_wakes_and_becomes_ready_when_value_ttl_expires() {
        let mut map = ExpiringHashMap::<String, String>::new();

        let ttl = Duration::from_secs(1);
        map.insert("key".to_owned(), "val".to_owned(), ttl);

        let mut fut = task::spawn(map.next_expired());

        // At first, has to be pending.
        assert_pending!(fut.poll());

        // Sleep twice the ttl, to guarantee we're over the deadline.
        assert_eq!(fut.is_woken(), false);
        tokio::time::delay_for(ttl * 2).await;
        assert_eq!(fut.is_woken(), true);

        // Then, after deadline, has to be ready.
        assert_eq!(
            unwrap_ready(fut.poll()).unwrap().unwrap().0,
            "val".to_owned()
        );
    }

    #[tokio::test]
    async fn next_expired_api_allows_inserting_items() {
        let mut map = ExpiringHashMap::<String, String>::new();

        // At first, has to be pending.
        let mut fut = task::spawn(map.next_expired());
        assert!(unwrap_ready(fut.poll()).is_none());
        drop(fut);

        // Insert an item.
        let ttl = Duration::from_secs(1000);
        map.insert("key".to_owned(), "val".to_owned(), ttl);

        // Then, after value is inserted, has to be still pending.
        let mut fut = task::spawn(map.next_expired());
        assert_pending!(fut.poll());
    }
}
