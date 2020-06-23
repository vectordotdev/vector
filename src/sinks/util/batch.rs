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
    pub fn use_size_as_bytes(&self) -> Result<Self, BatchError> {
        let max_bytes = match (self.max_bytes, self.max_size) {
            (Some(_), Some(_)) => return Err(BatchError::BytesAndSize),
            (Some(bytes), None) => Some(bytes),
            (None, Some(size)) => Some(size),
            (None, None) => None,
        };
        Ok(Self {
            max_bytes,
            max_size: None,
            ..*self
        })
    }

    pub fn use_size_as_events(&self) -> Result<Self, BatchError> {
        let max_events = match (self.max_events, self.max_size) {
            (Some(_), Some(_)) => return Err(BatchError::EventsAndSize),
            (Some(events), None) => Some(events),
            (None, Some(size)) => Some(size),
            (None, None) => None,
        };
        // Sinks that used `max_size` for an event count cannot count
        // bytes, so err if `max_bytes` is set.
        if self.max_bytes.is_some() {
            return Err(BatchError::BytesNotAllowed);
        }
        Ok(Self {
            max_events: max_events,
            max_size: None,
            ..*self
        })
    }

    pub fn get_settings_or_default(&self, defaults: BatchSettings) -> BatchSettings {
        BatchSettings {
            bytes: self.max_bytes.unwrap_or(defaults.bytes),
            events: self.max_events.unwrap_or(defaults.events),
            timeout: self
                .timeout_secs
                .map(|secs| Duration::from_secs(secs))
                .unwrap_or(defaults.timeout),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BatchSettings {
    pub bytes: usize,
    pub events: usize,
    pub timeout: Duration,
}

impl BatchSettings {
    // Fake the builder pattern
    pub fn bytes(self, bytes: u64) -> Self {
        Self {
            bytes: bytes as usize,
            ..self
        }
    }
    pub fn events(self, events: usize) -> Self {
        Self { events, ..self }
    }
    pub fn timeout(self, secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(secs),
            ..self
        }
    }
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
