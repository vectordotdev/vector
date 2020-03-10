use futures::stream::StreamExt;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::time::{Duration, Instant};
use tokio02::time::{delay_queue, DelayQueue, Error};

#[cfg(test)]
mod tests;

pub type ExpiredItem<K, V> = (V, delay_queue::Expired<K>);

pub struct ExpiringHashMap<K, V> {
    map: HashMap<K, (V, delay_queue::Key)>,
    expiration_queue: DelayQueue<K>,
}

impl<K, V> Unpin for ExpiringHashMap<K, V> {}

impl<K, V> ExpiringHashMap<K, V>
where
    K: Eq + Hash + Clone,
{
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            expiration_queue: DelayQueue::new(),
        }
    }

    pub fn insert(&mut self, key: K, value: V, ttl: Duration) {
        let delay_queue_key = self.expiration_queue.insert(key.clone(), ttl);
        self.map.insert(key, (value, delay_queue_key));
    }

    pub fn insert_at(&mut self, key: K, value: V, deadline: Instant) {
        let delay_queue_key = self
            .expiration_queue
            .insert_at(key.clone(), deadline.into());
        self.map.insert(key, (value, delay_queue_key));
    }

    pub fn get<Q>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.get(k).map(|&(ref v, _)| v)
    }

    pub fn get_mut<Q>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.get_mut(k).map(|&mut (ref mut v, _)| v)
    }

    pub fn reset_at<Q>(&mut self, k: &Q, when: Instant) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let (value, delay_queue_key) = self.map.get_mut(k)?;
        self.expiration_queue.reset_at(delay_queue_key, when.into());
        Some(value)
    }

    pub fn remove<Q>(&mut self, k: &Q) -> Option<ExpiredItem<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let (value, expiration_queue_key) = self.map.remove(k)?;
        let expired = self.expiration_queue.remove(&expiration_queue_key);
        Some((value, expired))
    }

    pub fn is_empty(&self) -> bool {
        self.expiration_queue.is_empty()
    }

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
