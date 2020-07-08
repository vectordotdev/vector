use derivative::Derivative;
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
    /// Deprecated. Left in for backwards compatibility, use `max_bytes`
    /// or `max_events` instead.
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

    pub fn disallow_max_bytes(&self) -> Result<Self, BatchError> {
        // Sinks that used `max_size` for an event count cannot count
        // bytes, so err if `max_bytes` is set.
        match self.max_bytes {
            Some(_) => Err(BatchError::BytesNotAllowed),
            None => Ok(*self),
        }
    }

    pub fn use_size_as_events(&self) -> Result<Self, BatchError> {
        let max_events = match (self.max_events, self.max_size) {
            (Some(_), Some(_)) => return Err(BatchError::EventsAndSize),
            (Some(events), None) => Some(events),
            (None, Some(size)) => Some(size),
            (None, None) => None,
        };
        Ok(Self {
            max_events,
            max_size: None,
            ..*self
        })
    }

    pub fn get_settings_or_default(&self, defaults: BatchSettings) -> BatchSettings {
        BatchSettings {
            size: BatchSize {
                bytes: self.max_bytes.unwrap_or(defaults.size.bytes),
                events: self.max_events.unwrap_or(defaults.size.events),
            },
            timeout: self
                .timeout_secs
                .map(Duration::from_secs)
                .unwrap_or(defaults.timeout),
        }
    }
}

#[derive(Clone, Copy, Debug, Derivative)]
#[derivative(Default)]
pub struct BatchSize {
    #[derivative(Default(value = "usize::max_value()"))]
    pub bytes: usize,
    #[derivative(Default(value = "usize::max_value()"))]
    pub events: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BatchSettings {
    pub size: BatchSize,
    pub timeout: Duration,
}

impl BatchSettings {
    // Fake the builder pattern
    pub const fn bytes(self, bytes: u64) -> Self {
        Self {
            size: BatchSize {
                bytes: bytes as usize,
                ..self.size
            },
            ..self
        }
    }
    pub const fn events(self, events: usize) -> Self {
        Self {
            size: BatchSize {
                events,
                ..self.size
            },
            ..self
        }
    }
    pub const fn timeout(self, secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(secs),
            ..self
        }
    }
}

pub(super) fn err_event_too_large<T>(length: usize) -> PushResult<T> {
    error!(message = "Event larger than batch size, dropping", %length, rate_limit_secs = 1);
    PushResult::Ok(false)
}

/// This enum provides the result of a push operation, indicating if the
/// event was added and the fullness state of the buffer.
#[must_use]
#[derive(Debug, Eq, PartialEq)]
pub enum PushResult<T> {
    /// Event was added, with an indicator if the buffer is now full
    Ok(bool),
    /// Event could not be added because it would overflow the
    /// buffer. Since push takes ownership of the event, it must be
    /// returned here.
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
            self.was_full =
                matches!(result, PushResult::Overflow(_)) || matches!(result, PushResult::Ok(true));
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
