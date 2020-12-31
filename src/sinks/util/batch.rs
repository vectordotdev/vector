use derivative::Derivative;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::marker::PhantomData;
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
    // This is used internally by new_relic_logs sink, else it could be pub(super) too
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

    pub(super) fn disallow_max_bytes(&self) -> Result<Self, BatchError> {
        // Sinks that used `max_size` for an event count cannot count
        // bytes, so err if `max_bytes` is set.
        match self.max_bytes {
            Some(_) => Err(BatchError::BytesNotAllowed),
            None => Ok(*self),
        }
    }

    pub(super) fn use_size_as_events(&self) -> Result<Self, BatchError> {
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

    pub(super) fn get_settings_or_default<T>(
        &self,
        defaults: BatchSettings<T>,
    ) -> BatchSettings<T> {
        BatchSettings {
            size: BatchSize {
                bytes: self.max_bytes.unwrap_or(defaults.size.bytes),
                events: self.max_events.unwrap_or(defaults.size.events),
                ..Default::default()
            },
            timeout: self
                .timeout_secs
                .map(Duration::from_secs)
                .unwrap_or(defaults.timeout),
        }
    }
}

#[derive(Debug, Derivative)]
#[derivative(Clone(bound = ""))]
#[derivative(Copy(bound = ""))]
#[derivative(Default(bound = ""))]
pub struct BatchSize<B> {
    #[derivative(Default(value = "usize::max_value()"))]
    pub bytes: usize,
    #[derivative(Default(value = "usize::max_value()"))]
    pub events: usize,
    // This type marker is used to drive type inference, which allows us
    // to call the right Batch::get_settings_defaults without explicitly
    // naming the type in BatchSettings::parse_config.
    _type_marker: PhantomData<B>,
}

#[derive(Debug, Derivative)]
#[derivative(Clone(bound = ""))]
#[derivative(Copy(bound = ""))]
#[derivative(Default(bound = ""))]
pub struct BatchSettings<B> {
    pub size: BatchSize<B>,
    pub timeout: Duration,
}

impl<B: Batch> BatchSettings<B> {
    pub fn parse_config(self, config: BatchConfig) -> Result<Self, BatchError> {
        B::get_settings_defaults(config, self)
    }
}

impl<B> BatchSettings<B> {
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

    // Would like to use `trait From` here, but that results in
    // "conflicting implementations of trait"
    pub const fn into<B2>(self) -> BatchSettings<B2> {
        BatchSettings {
            size: BatchSize {
                bytes: self.size.bytes,
                events: self.size.events,
                _type_marker: PhantomData,
            },
            timeout: self.timeout,
        }
    }
}

pub(super) fn err_event_too_large<T>(length: usize) -> PushResult<T> {
    error!(message = "Event larger than batch size, dropping.", length = %length, internal_log_rate_secs = 1);
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

pub trait Batch: Sized {
    type Input;
    type Output;

    /// Turn the batch configuration into an actualized set of settings,
    /// and deal with the proper behavior of `max_size` and if
    /// `max_bytes` may be set. This is in the trait to ensure all batch
    /// buffers implement it.
    fn get_settings_defaults(
        _config: BatchConfig,
        _defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError>;

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

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(B::get_settings_defaults(config, defaults.into())?.into())
    }

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
