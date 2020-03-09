use futures::ready;
use futures::stream::Stream;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio02::time::{delay_queue, DelayQueue, Error};

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

    pub fn insert(&mut self, key: K, value_with_ttl: (V, Duration)) {
        let (value, ttl) = value_with_ttl;
        let delay = self.expiration_queue.insert(key.clone(), ttl);
        self.map.insert(key, (value, delay));
    }

    pub fn insert_at(&mut self, key: K, value_with_deadline: (V, Instant)) {
        let (value, deadline) = value_with_deadline;
        let delay = self
            .expiration_queue
            .insert_at(key.clone(), deadline.into());
        self.map.insert(key, (value, delay));
    }

    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.get(k).map(|&(ref v, _)| v)
    }

    pub fn get_mut<Q: ?Sized>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.get_mut(k).map(|&mut (ref mut v, _)| v)
    }

    pub fn reset_at<Q: ?Sized>(&mut self, k: &Q, when: Instant) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        let (value, delay_queue_key) = self.map.get_mut(k)?;
        self.expiration_queue.reset_at(delay_queue_key, when.into());
        Some(value)
    }

    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<(V, delay_queue::Expired<K>)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        let (value, expiration_queue_key) = self.map.remove(k)?;
        let expired = self.expiration_queue.remove(&expiration_queue_key);
        Some((value, expired))
    }

    pub fn poll_expired(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<ExpiredItem<K, V>, Error>>> {
        let key = ready!(self.expiration_queue.poll_expired(cx));
        let key = match key {
            None => return Poll::Ready(None),
            Some(Err(err)) => return Poll::Ready(Some(Err(err))),
            Some(Ok(key)) => key,
        };
        let (value, _) = self.map.remove(key.get_ref()).unwrap();
        Poll::Ready(Some(Ok((value, key))))
    }
}

impl<K, V> Stream for ExpiringHashMap<K, V>
where
    K: Eq + Hash + Clone,
{
    type Item = Result<ExpiredItem<K, V>, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(self).poll_expired(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.expiration_queue.size_hint()
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
