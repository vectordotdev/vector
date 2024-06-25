use std::{marker::PhantomData, num::NonZeroUsize, time::Duration};

use derivative::Derivative;
use serde_with::serde_as;
use snafu::Snafu;
use vector_lib::configurable::configurable_component;
use vector_lib::json_size::JsonSize;
use vector_lib::stream::BatcherSettings;

use super::EncodedEvent;
use crate::{event::EventFinalizers, internal_events::LargeEventDroppedError};

// * Provide sensible sink default 10 MB with 1s timeout. Don't allow chaining builder methods on
//   that.

#[derive(Debug, Snafu, PartialEq, Eq)]
pub enum BatchError {
    #[snafu(display("This sink does not allow setting `max_bytes`"))]
    BytesNotAllowed,
    #[snafu(display("`max_bytes` must be greater than zero"))]
    InvalidMaxBytes,
    #[snafu(display("`max_events` must be greater than zero"))]
    InvalidMaxEvents,
    #[snafu(display("`timeout_secs` must be greater than zero"))]
    InvalidTimeout,
    #[snafu(display("provided `max_bytes` exceeds the maximum limit of {}", limit))]
    MaxBytesExceeded { limit: usize },
    #[snafu(display("provided `max_events` exceeds the maximum limit of {}", limit))]
    MaxEventsExceeded { limit: usize },
}

pub trait SinkBatchSettings {
    const MAX_EVENTS: Option<usize>;
    const MAX_BYTES: Option<usize>;
    const TIMEOUT_SECS: f64;
}

/// Reasonable default batch settings for sinks with timeliness concerns, limited by event count.
#[derive(Clone, Copy, Debug, Default)]
pub struct RealtimeEventBasedDefaultBatchSettings;

impl SinkBatchSettings for RealtimeEventBasedDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1000);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Reasonable default batch settings for sinks with timeliness concerns, limited by byte size.
#[derive(Clone, Copy, Debug, Default)]
pub struct RealtimeSizeBasedDefaultBatchSettings;

impl SinkBatchSettings for RealtimeSizeBasedDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(10_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Reasonable default batch settings for sinks focused on shipping fewer-but-larger batches,
/// limited by byte size.
#[derive(Clone, Copy, Debug, Default)]
pub struct BulkSizeBasedDefaultBatchSettings;

impl SinkBatchSettings for BulkSizeBasedDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(10_000_000);
    const TIMEOUT_SECS: f64 = 300.0;
}

/// "Default" batch settings when a sink handles batch settings entirely on its own.
///
/// This has very few usages, but can be notably seen in the Kafka sink, where the values are used
/// to configure `librdkafka` itself rather than being passed as `BatchSettings`/`BatcherSettings`
/// to components in the sink itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoDefaultsBatchSettings;

impl SinkBatchSettings for NoDefaultsBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Merged;

#[derive(Clone, Copy, Debug, Default)]
pub struct Unmerged;

/// Event batching behavior.
// NOTE: the default values are extracted from the consts in `D`. This generates correct defaults
// in automatic cue docs generation. Implementations of `SinkBatchSettings` should not specify
// defaults, since that is satisfied here.
#[serde_as]
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Copy, Debug, Default)]
pub struct BatchConfig<D: SinkBatchSettings + Clone, S = Unmerged>
where
    S: Clone,
{
    /// The maximum size of a batch that is processed by a sink.
    ///
    /// This is based on the uncompressed size of the batched events, before they are
    /// serialized/compressed.
    #[serde(default = "default_max_bytes::<D>")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_bytes: Option<usize>,

    /// The maximum size of a batch before it is flushed.
    #[serde(default = "default_max_events::<D>")]
    #[configurable(metadata(docs::type_unit = "events"))]
    pub max_events: Option<usize>,

    /// The maximum age of a batch before it is flushed.
    #[serde(default = "default_timeout::<D>")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Timeout"))]
    pub timeout_secs: Option<f64>,

    #[serde(skip)]
    _d: PhantomData<D>,
    #[serde(skip)]
    _s: PhantomData<S>,
}

const fn default_max_bytes<D: SinkBatchSettings>() -> Option<usize> {
    D::MAX_BYTES
}

const fn default_max_events<D: SinkBatchSettings>() -> Option<usize> {
    D::MAX_EVENTS
}

const fn default_timeout<D: SinkBatchSettings>() -> Option<f64> {
    Some(D::TIMEOUT_SECS)
}

impl<D: SinkBatchSettings + Clone> BatchConfig<D, Unmerged> {
    pub fn validate(self) -> Result<BatchConfig<D, Merged>, BatchError> {
        let config = BatchConfig {
            max_bytes: self.max_bytes.or(D::MAX_BYTES),
            max_events: self.max_events.or(D::MAX_EVENTS),
            timeout_secs: self.timeout_secs.or(Some(D::TIMEOUT_SECS)),
            _d: PhantomData,
            _s: PhantomData,
        };

        match (config.max_bytes, config.max_events, config.timeout_secs) {
            // TODO: what logic do we want to check that we have the minimum number of settings?
            // for example, we always assert that timeout_secs from D is greater than zero, but
            // technically we could end up with max bytes or max events being none, since we just
            // chain options... but asserting that they're set isn't really doable either, because
            // you dont always set both of those fields, etc..
            (Some(0), _, _) => Err(BatchError::InvalidMaxBytes),
            (_, Some(0), _) => Err(BatchError::InvalidMaxEvents),
            (_, _, Some(timeout)) if timeout <= 0.0 => Err(BatchError::InvalidTimeout),

            _ => Ok(config),
        }
    }

    pub fn into_batch_settings<T: Batch>(self) -> Result<BatchSettings<T>, BatchError> {
        let config = self.validate()?;
        config.into_batch_settings()
    }

    /// Converts these settings into [`BatcherSettings`].
    ///
    /// `BatcherSettings` is effectively the `vector_core` spiritual successor of
    /// [`BatchSettings<B>`].  Once all sinks are rewritten in the new stream-based style and we can
    /// eschew customized batch buffer types, we can de-genericify `BatchSettings` and move it into
    /// `vector_core`, and use that instead of `BatcherSettings`.
    pub fn into_batcher_settings(self) -> Result<BatcherSettings, BatchError> {
        let config = self.validate()?;
        config.into_batcher_settings()
    }
}

impl<D: SinkBatchSettings + Clone> BatchConfig<D, Merged> {
    pub const fn validate(self) -> Result<BatchConfig<D, Merged>, BatchError> {
        Ok(self)
    }

    pub const fn disallow_max_bytes(self) -> Result<Self, BatchError> {
        // Sinks that used `max_size` for an event count cannot count
        // bytes, so err if `max_bytes` is set.
        match self.max_bytes {
            Some(_) => Err(BatchError::BytesNotAllowed),
            None => Ok(self),
        }
    }

    pub const fn limit_max_bytes(self, limit: usize) -> Result<Self, BatchError> {
        match self.max_bytes {
            Some(n) if n > limit => Err(BatchError::MaxBytesExceeded { limit }),
            _ => Ok(self),
        }
    }

    pub const fn limit_max_events(self, limit: usize) -> Result<Self, BatchError> {
        match self.max_events {
            Some(n) if n > limit => Err(BatchError::MaxEventsExceeded { limit }),
            _ => Ok(self),
        }
    }

    pub fn into_batch_settings<T: Batch>(self) -> Result<BatchSettings<T>, BatchError> {
        let adjusted = T::get_settings_defaults(self)?;

        // This is unfortunate since we technically have already made sure this isn't possible in
        // `validate`, but alas.
        let timeout_secs = adjusted.timeout_secs.ok_or(BatchError::InvalidTimeout)?;

        Ok(BatchSettings {
            size: BatchSize {
                bytes: adjusted.max_bytes.unwrap_or(usize::MAX),
                events: adjusted.max_events.unwrap_or(usize::MAX),
                _type_marker: PhantomData,
            },
            timeout: Duration::from_secs_f64(timeout_secs),
        })
    }

    /// Converts these settings into [`BatcherSettings`].
    ///
    /// `BatcherSettings` is effectively the `vector_core` spiritual successor of
    /// [`BatchSettings<B>`].  Once all sinks are rewritten in the new stream-based style and we can
    /// eschew customized batch buffer types, we can de-genericify `BatchSettings` and move it into
    /// `vector_core`, and use that instead of `BatcherSettings`.
    pub fn into_batcher_settings(self) -> Result<BatcherSettings, BatchError> {
        let max_bytes = self
            .max_bytes
            .and_then(NonZeroUsize::new)
            .or_else(|| NonZeroUsize::new(usize::MAX))
            .expect("`max_bytes` should already be validated");

        let max_events = self
            .max_events
            .and_then(NonZeroUsize::new)
            .or_else(|| NonZeroUsize::new(usize::MAX))
            .expect("`max_bytes` should already be validated");

        // This is unfortunate since we technically have already made sure this isn't possible in
        // `validate`, but alas.
        let timeout_secs = self.timeout_secs.ok_or(BatchError::InvalidTimeout)?;

        Ok(BatcherSettings::new(
            Duration::from_secs_f64(timeout_secs),
            max_bytes,
            max_events,
        ))
    }
}

// Going from a merged to unmerged configuration is fine, because we know it already had to have
// been validated/limited.
impl<D1, D2> From<BatchConfig<D1, Merged>> for BatchConfig<D2, Unmerged>
where
    D1: SinkBatchSettings + Clone,
    D2: SinkBatchSettings + Clone,
{
    fn from(config: BatchConfig<D1, Merged>) -> Self {
        BatchConfig {
            max_bytes: config.max_bytes,
            max_events: config.max_events,
            timeout_secs: config.timeout_secs,
            _d: PhantomData,
            _s: PhantomData,
        }
    }
}

#[derive(Debug, Derivative)]
#[derivative(Clone(bound = ""))]
#[derivative(Copy(bound = ""))]
pub struct BatchSize<B> {
    pub bytes: usize,
    pub events: usize,
    // This type marker is used to drive type inference, which allows us
    // to call the right Batch::get_settings_defaults without explicitly
    // naming the type in BatchSettings::parse_config.
    _type_marker: PhantomData<B>,
}

impl<B> BatchSize<B> {
    pub const fn const_default() -> Self {
        BatchSize {
            bytes: usize::MAX,
            events: usize::MAX,
            _type_marker: PhantomData,
        }
    }
}

impl<B> Default for BatchSize<B> {
    fn default() -> Self {
        BatchSize::const_default()
    }
}

#[derive(Debug, Derivative)]
#[derivative(Clone(bound = ""))]
#[derivative(Copy(bound = ""))]
pub struct BatchSettings<B> {
    pub size: BatchSize<B>,
    pub timeout: Duration,
}

impl<B> Default for BatchSettings<B> {
    fn default() -> Self {
        BatchSettings {
            size: BatchSize {
                bytes: 10_000_000,
                events: usize::MAX,
                _type_marker: PhantomData,
            },
            timeout: Duration::from_secs(1),
        }
    }
}

pub(super) fn err_event_too_large<T>(length: usize, max_length: usize) -> PushResult<T> {
    emit!(LargeEventDroppedError { length, max_length });
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
    fn get_settings_defaults<D: SinkBatchSettings + Clone>(
        config: BatchConfig<D, Merged>,
    ) -> Result<BatchConfig<D, Merged>, BatchError> {
        Ok(config)
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input>;
    fn is_empty(&self) -> bool;
    fn fresh(&self) -> Self;
    fn finish(self) -> Self::Output;
    fn num_items(&self) -> usize;
}

#[derive(Debug)]
pub struct EncodedBatch<I> {
    pub items: I,
    pub finalizers: EventFinalizers,
    pub count: usize,
    pub byte_size: usize,
    pub json_byte_size: JsonSize,
}

/// This is a batch construct that stores an set of event finalizers alongside the batch itself.
#[derive(Clone, Debug)]
pub struct FinalizersBatch<B> {
    inner: B,
    finalizers: EventFinalizers,
    // The count of items inserted into this batch is distinct from the
    // number of items recorded by the inner batch, as that inner count
    // could be smaller due to aggregated items (ie metrics).
    count: usize,
    byte_size: usize,
    json_byte_size: JsonSize,
}

impl<B: Batch> From<B> for FinalizersBatch<B> {
    fn from(inner: B) -> Self {
        Self {
            inner,
            finalizers: Default::default(),
            count: 0,
            byte_size: 0,
            json_byte_size: JsonSize::zero(),
        }
    }
}

impl<B: Batch> Batch for FinalizersBatch<B> {
    type Input = EncodedEvent<B::Input>;
    type Output = EncodedBatch<B::Output>;

    fn get_settings_defaults<D: SinkBatchSettings + Clone>(
        config: BatchConfig<D, Merged>,
    ) -> Result<BatchConfig<D, Merged>, BatchError> {
        B::get_settings_defaults(config)
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        let EncodedEvent {
            item,
            finalizers,
            byte_size,
            json_byte_size,
        } = item;
        match self.inner.push(item) {
            PushResult::Ok(full) => {
                self.finalizers.merge(finalizers);
                self.count += 1;
                self.byte_size += byte_size;
                self.json_byte_size += json_byte_size;
                PushResult::Ok(full)
            }
            PushResult::Overflow(item) => PushResult::Overflow(EncodedEvent {
                item,
                finalizers,
                byte_size,
                json_byte_size,
            }),
        }
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn fresh(&self) -> Self {
        Self {
            inner: self.inner.fresh(),
            finalizers: Default::default(),
            count: 0,
            byte_size: 0,
            json_byte_size: JsonSize::zero(),
        }
    }

    fn finish(self) -> Self::Output {
        EncodedBatch {
            items: self.inner.finish(),
            finalizers: self.finalizers,
            count: self.count,
            byte_size: self.byte_size,
            json_byte_size: self.json_byte_size,
        }
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}

#[derive(Clone, Debug)]
pub struct StatefulBatch<B> {
    inner: B,
    was_full: bool,
}

impl<B: Batch> From<B> for StatefulBatch<B> {
    fn from(inner: B) -> Self {
        Self {
            inner,
            was_full: false,
        }
    }
}

impl<B> StatefulBatch<B> {
    pub const fn was_full(&self) -> bool {
        self.was_full
    }

    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn into_inner(self) -> B {
        self.inner
    }
}

impl<B: Batch> Batch for StatefulBatch<B> {
    type Input = B::Input;
    type Output = B::Output;

    fn get_settings_defaults<D: SinkBatchSettings + Clone>(
        config: BatchConfig<D, Merged>,
    ) -> Result<BatchConfig<D, Merged>, BatchError> {
        B::get_settings_defaults(config)
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
