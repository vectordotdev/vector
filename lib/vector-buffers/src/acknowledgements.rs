use std::{
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

/// A value that can be acknowledged.
///
/// This is used to define how many events should be acknowledged when this value has been
/// processed.  Since the value might be tied to a single event, or to multiple events, this
/// provides a generic mechanism for gathering the number of events to acknowledge.
pub trait Ackable {
    /// Number of events to acknowledge for this value.
    fn ack_size(&self) -> usize;
}

/// A handle for acknowledging reads from a buffer.
///
/// In many cases, acknowledgements (sometimes referred to as "end-to-end acknowledgements") flow
/// with an event from the moment it is processed by a source, all the way until it is processed by
/// a sink.  This occurs with in-memory buffers, which are the default, as the `Event` object itself
/// which carries the acknowledgement state is never serialized or deserialized.
///
/// In other cases, such as disk buffers, the act of serializing the event to disk inherently strips
/// the acknowledgement data from the event that gets deserialized on the other side.  When an event
/// is written to a disk buffer, we acknowledge it after it has been written, which is where we know
/// that the event has been durably stored to disk and the source can propagate that acknowledgement
/// information as needed.
///
/// The other side of the equation is when we read events back out of the disk buffer and process
/// them in a sink.  Similar to if there was no disk buffer, we want the "source" -- which is the
/// disk buffer itself -- to know when it can remove the event from disk after it has been durably
/// processed by the downstream sink, and this is where `Acker` comes into play.
///
/// Any sink using a buffer is given an `Acker` that they use to update the acknowledgement state as
/// they process events.  Even when an in-memory buffer is used, `Acker` is still called, even
/// though it is a no-op.  In this sense, the `passthrough` variant represents buffers that do not
/// otherwise break the finalization logic when the event crosses into the buffer.
///
/// Conversely, `segmented` represents buffers where the acknowledgement logic is segmented into
/// multiple parts: the acknowledgement when being written to the buffer, and the secondary
/// acknowledgement when being processed by the sink after reading out of the buffer.
#[derive(Clone)]
pub struct Acker {
    inner: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl Acker {
    /// Creates a passthrough [`Acker`].
    ///
    /// All calls to [`ack`] are a no-op.
    pub fn passthrough() -> Self {
        Self { inner: None }
    }

    /// Creates a segmented [`Acker`].
    ///
    /// All calls to [`ack`] will call the given function `f`.
    pub fn segmented<F>(f: F) -> Self
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        Self {
            inner: Some(Arc::new(f)),
        }
    }

    /// Creates a basic [`Acker`] that simply tracks the total acknowledgement count.
    ///
    /// A handle to the underlying [`AtomicUsize`] is passed back alongside the `Acker` itself.
    pub fn basic() -> (Self, Arc<AtomicUsize>) {
        let counter = Arc::new(AtomicUsize::new(0));

        let counter2 = Arc::clone(&counter);
        let inner = move |num: usize| {
            counter2.fetch_add(num, Ordering::Relaxed);
        };
        let acker = Acker::segmented(inner);

        (acker, counter)
    }

    /// Acknowledge a certain amount of records.
    ///
    /// Callers are responsible for ensuring that acknowledgements are in order.  That is to say, if
    /// multiple records are read from the buffer, and all messages except one are durably processed, only
    /// the count of the messages processed up until that message can be acknowledged.
    pub fn ack(&self, num: usize) {
        if num > 0 {
            if let Some(inner) = self.inner.as_ref() {
                (&*inner)(num);
            }
        }
    }
}

impl fmt::Debug for Acker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Acker")
            .field(
                "inner",
                if self.inner.is_some() {
                    &"segmented"
                } else {
                    &"passthrough"
                },
            )
            .finish()
    }
}

impl<T> Ackable for Vec<T>
where
    T: Ackable,
{
    fn ack_size(&self) -> usize {
        self.iter().map(Ackable::ack_size).sum()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use crate::Acker;

    #[test]
    fn basic() {
        let (acker, counter) = Acker::basic();

        assert_eq!(0, counter.load(Ordering::Relaxed));

        acker.ack(0);
        assert_eq!(0, counter.load(Ordering::Relaxed));

        acker.ack(42);
        assert_eq!(42, counter.load(Ordering::Relaxed));
    }
}
