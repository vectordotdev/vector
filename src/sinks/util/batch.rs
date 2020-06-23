use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::time::Duration;

#[derive(Debug, Snafu)]
pub enum BatchError {
    #[snafu(display("Cannot configure both `max_bytes` and `max_size`"))]
    BytesAndSize,
    #[snafu(display("Cannot configure both `max_events` and `max_size`"))]
    EventsAndSize,
    #[snafu(display("This sink does not allow setting `max_bytes`"))]
    BytesNotAllowed,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct BatchConfig {
    pub max_bytes: Option<usize>,
    pub max_events: Option<usize>,
    // This field is deprecated, left in for backwards compatibility
    pub max_size: Option<usize>,
    pub timeout_secs: Option<u64>,
}

impl BatchConfig {
    pub fn get_bytes_or(&self, bytes: u64) -> Result<usize, BatchError> {
        match (self.max_bytes, self.max_size) {
            (Some(_), Some(_)) => Err(BatchError::BytesAndSize),
            (Some(bytes), None) => Ok(bytes),
            (None, Some(size)) => Ok(size),
            (None, None) => Ok(bytes as usize),
        }
    }

    pub fn parse_with_bytes(
        &self,
        bytes: u64,
        events: usize,
        timeout: u64,
    ) -> Result<BatchSettings, BatchError> {
        Ok(BatchSettings {
            bytes: self.get_bytes_or(bytes)?,
            events: self.max_events.unwrap_or(events),
            timeout: Duration::from_secs(self.timeout_secs.unwrap_or(timeout)),
        })
    }

    pub fn get_events_or(&self, events: usize) -> Result<usize, BatchError> {
        match (self.max_events, self.max_size) {
            (Some(_), Some(_)) => Err(BatchError::EventsAndSize),
            (Some(events), None) => Ok(events),
            (None, Some(size)) => Ok(size),
            (None, None) => Ok(events),
        }
    }

    pub fn parse_with_events(
        &self,
        bytes: u64,
        events: usize,
        timeout: u64,
    ) -> Result<BatchSettings, BatchError> {
        if bytes == 0 && self.max_bytes.is_some() {
            return Err(BatchError::BytesNotAllowed);
        }
        Ok(BatchSettings {
            bytes: self.max_bytes.unwrap_or(bytes as usize),
            events: self.get_events_or(events)?,
            timeout: Duration::from_secs(self.timeout_secs.unwrap_or(timeout)),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BatchSettings {
    pub bytes: usize,
    pub events: usize,
    pub timeout: Duration,
}

#[must_use]
#[derive(Debug, Eq, PartialEq)]
pub enum PushResult<T> {
    Ok,
    Overflow(T),
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
            PushResult::Overflow(item)
        } else {
            let result = self.inner.push(item);
            self.was_full = matches!(result, PushResult::Overflow(_));
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
