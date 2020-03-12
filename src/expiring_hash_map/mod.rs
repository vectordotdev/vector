//! Expiring Hash Map and related types. See [`ExpiringHashMap`].
#![warn(missing_docs)]

use futures::stream::StreamExt;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::time::{Duration, Instant};
use tokio02::time::{delay_queue, DelayQueue, Error};

#[cfg(test)]
mod tests;

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

    /// Reset the dealine for a key, and return a mut ref to the value.
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
    /// expiried items regardless of what state the [`ExpiringHashMap`] is.
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
    ///    wait indefinetly - or until an item is inserted. This would be
    ///    possible to carry on, for instance, from a sibling branch of a
    ///    `select` statement, so the borrowing rules won't be a problem here.
    /// 3. We went over the following alternative signature:
    ///    ```ignore
    ///    pub fn next_expired(&mut self) -> Option<impl Future<Outcome = Result<ExpiredItem<K, V>, Error>>> {...}
    ///    ```
    ///    This captures the API restrictions a bit better, and should provide
    ///    less possibilities to misuse the API.
    ///    We didn't pick this one becuase it's not an `async fn` and we wanted
    ///    this, see (1) of this list. Furtherore, instead of doing a
    ///    `select { _ = map.next_expired(), if !map.is_empty() => { ... } }`
    ///    users would have to do
    ///    `let exp = map.next_expired(); select { _ = exp.unwrap(), if exp.is_some() => { ... } }`,
    ///    which is less readable and a bit harder to understand. Although it
    ///    has a possiblity of a nicer generalization if `select` macro
    ///    supported a `Some(future)` kind of pattern matching, we decided to go
    ///    with other solution for now.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio02 as tokio;
    /// # let mut rt = tokio::runtime::Runtime::new().unwrap();
    /// # rt.block_on(async {
    /// use vector::expiring_hash_map::ExpiringHashMap;
    /// use std::time::Duration;
    ///
    /// let mut map: ExpiringHashMap<String, String> = ExpiringHashMap::new();
    ///
    /// loop {
    ///     tokio::select! {
    ///         // Avoid spin lock!
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
