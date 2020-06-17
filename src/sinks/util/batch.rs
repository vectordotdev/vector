use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct BatchBytesConfig {
    pub max_size: Option<usize>,
    pub timeout_secs: Option<u64>,
}

impl BatchBytesConfig {
    pub fn unwrap_or(&self, size: u64, timeout: u64) -> BatchSettings {
        BatchSettings {
            size: self.max_size.unwrap_or(size as usize),
            timeout: Duration::from_secs(self.timeout_secs.unwrap_or(timeout)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct BatchEventsConfig {
    pub max_events: Option<usize>,
    pub timeout_secs: Option<u64>,
}

impl BatchEventsConfig {
    pub fn unwrap_or(&self, size: u64, timeout: u64) -> BatchSettings {
        BatchSettings {
            size: self.max_events.unwrap_or(size as usize),
            timeout: Duration::from_secs(self.timeout_secs.unwrap_or(timeout)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BatchSettings {
    pub size: usize,
    pub timeout: Duration,
}

#[must_use]
#[derive(Debug, Eq, PartialEq)]
pub enum PushResult<T> {
    Ok,
    Full(T),
}

pub trait Batch {
    type Input;
    type Output;

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input>;
    fn is_empty(&self) -> bool;
    fn fresh(&self) -> Self;
    fn finish(self) -> Self::Output;
    fn num_items(&self) -> usize;

    /// Replace the current batch with a fresh one, returning the old one.
    fn fresh_replace(&mut self) -> Self
    where
        Self: Sized,
    {
        let fresh = self.fresh();
        std::mem::replace(self, fresh)
    }
}

#[derive(Clone, Debug)]
pub struct StatefulBatch<B> {
    inner: B,
    was_full: bool,
}

impl<B> From<B> for StatefulBatch<B> {
    fn from(inner: B) -> Self {
        Self {
            inner,
            was_full: false,
        }
    }
}

impl<B> StatefulBatch<B> {
    pub fn was_full(&self) -> bool {
        self.was_full
    }

    pub fn into_inner(self) -> B {
        self.inner
    }
}

impl<B> Batch for StatefulBatch<B>
where
    B: Batch,
{
    type Input = B::Input;
    type Output = B::Output;

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        if self.was_full {
            PushResult::Full(item)
        } else {
            let result = self.inner.push(item);
            self.was_full = matches!(result, PushResult::Full(_));
            result
        }
    }

    fn is_empty(&self) -> bool {
        !self.was_full && self.inner.is_empty()
    }

    fn fresh(&self) -> Self {
        Self {
            inner: self.inner.fresh(),
            was_full: false,
        }
    }

    fn finish(self) -> Self::Output {
        self.inner.finish()
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}
