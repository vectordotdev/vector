mod in_memory;
#[cfg(feature = "disk-buffer")]
mod on_disk;

use crate::test::common::{Action, Message};
use crate::test::model::in_memory::InMemory;
#[cfg(feature = "disk-buffer")]
use crate::test::model::on_disk::OnDisk;
use crate::Variant;
use futures::task::{noop_waker, Context, Poll};
use futures::{Sink, Stream};
use quickcheck::{QuickCheck, TestResult};
use std::pin::Pin;
use tracing::Span;

#[derive(Debug)]
/// For operations that might block whether the operation would or would not
/// have, as the models should never interrupt program flow.
enum Progress {
    /// Operation did "block", passes back the `Message`
    Blocked(Message),
    /// Operation did "advance"
    Advanced,
}

/// A common trait for our "model", the "obviously correct" counterpart to the
/// system under test
trait Model {
    fn send(&mut self, item: Message) -> Progress;
    fn recv(&mut self) -> Option<Message>;
    fn is_full(&self) -> bool;
    fn is_empty(&self) -> bool;
}

fn check(variant: &Variant) -> bool {
    match variant {
        Variant::Memory { .. } => {
            // nothing to check
            true
        }
        #[cfg(feature = "disk-buffer")]
        Variant::Disk { id, data_dir, .. } => {
            // determine if data_dir is in temp_dir/id
            let mut prefix = std::path::PathBuf::new();
            prefix.push(std::env::temp_dir());
            prefix.push(id);

            data_dir.starts_with(prefix)
        }
    }
}

/// `VariantGuard` wraps a `Variant`, allowing a convenient Drop implementation
struct VariantGuard {
    inner: Variant,
}

impl VariantGuard {
    fn new(variant: Variant) -> Self {
        match variant {
            Variant::Memory { .. } => VariantGuard { inner: variant },
            #[cfg(feature = "disk-buffer")]
            Variant::Disk {
                max_size,
                when_full,
                id,
                ..
            } => {
                // SAFETY: We allow tempdir to create the directory but by
                // calling `into_path` we obligate ourselves to delete it. This
                // is done in the drop implementation for `VariantGuard`.
                let data_dir = tempdir::TempDir::new_in(std::env::temp_dir(), &id)
                    .unwrap()
                    .into_path();
                VariantGuard {
                    inner: Variant::Disk {
                        max_size,
                        when_full,
                        data_dir,
                        id,
                    },
                }
            }
        }
    }
}

impl AsRef<Variant> for VariantGuard {
    fn as_ref(&self) -> &Variant {
        &self.inner
    }
}

impl Drop for VariantGuard {
    fn drop(&mut self) {
        match &self.inner {
            Variant::Memory { .. } => { /* nothing to clean up */ }
            #[cfg(feature = "disk-buffer")]
            Variant::Disk { data_dir, .. } => {
                // SAFETY: Here we clean up the data_dir of the inner `Variant`,
                // see note in the constructor for this type.
                std::fs::remove_dir_all(data_dir).unwrap();
            }
        }
    }
}

/// This test models a single sender and a single receiver pushing and pulling
/// from a common buffer. The buffer itself may be either memory or disk. We use
/// the raw `futures::sink::Sink` and `futures::stream::Stream` interface,
/// avoiding the need to model the runtime in any way. This is, then, the buffer
/// as a runtime will see it.
///
/// Acks are not modeled yet. I believe doing so would be a straightforward
/// process.
#[test]
fn model_check() {
    fn inner(variant: Variant, actions: Vec<Action>) -> TestResult {
        if !check(&variant) {
            return TestResult::discard();
        }

        let guard = VariantGuard::new(variant);
        let mut model: Box<dyn Model> = match guard.as_ref() {
            Variant::Memory { .. } => Box::new(InMemory::new(guard.as_ref(), 1)),
            #[cfg(feature = "disk-buffer")]
            Variant::Disk { .. } => Box::new(OnDisk::new(guard.as_ref())),
        };

        let rcv_waker = noop_waker();
        let mut rcv_context = Context::from_waker(&rcv_waker);

        let snd_waker = noop_waker();
        let mut snd_context = Context::from_waker(&snd_waker);

        let (tx, mut rx, _) =
            crate::build::<Message>(guard.as_ref().clone(), Span::none()).unwrap();

        let mut tx = tx.get();
        let sink = tx.as_mut();

        for action in actions {
            match action {
                // For each send action we attempt to send into the buffer and
                // if the buffer signals itself ready do the send, then
                // flush. We might profitably model a distinct flush action but
                // at the time this model was created there was no clear reason
                // to do so.
                Action::Send(msg) => match Sink::poll_ready(Pin::new(sink), &mut snd_context) {
                    Poll::Ready(Ok(())) => {
                        // Once the buffer signals its ready we are allowed to
                        // call `start_send`. The buffer may or may not make the
                        // value immediately available to a receiver, something
                        // we elide by immediately flushing.
                        assert_eq!(Ok(()), Sink::start_send(Pin::new(sink), msg.clone()));
                        assert!(matches!(model.send(msg.clone()), Progress::Advanced));
                        match Sink::poll_flush(Pin::new(sink), &mut snd_context) {
                            Poll::Ready(Ok(())) => {}
                            // If the buffer signals Ready/Ok then we're good to
                            // go. Both the model and the SUT will have received
                            // their item. However, if the SUT signals Pending
                            // then this is only valid so long as the model is
                            // full.
                            Poll::Pending => {
                                debug_assert!(model.is_full(), "{:?}", msg);
                            }
                            // The SUT must never signal an error when we
                            // flush. There is no way to recover from an error.
                            Poll::Ready(Err(_)) => return TestResult::failed(),
                        }
                    }
                    Poll::Pending => assert!(model.is_full()),
                    Poll::Ready(Err(_)) => return TestResult::failed(),
                },
                Action::Recv => match Stream::poll_next(Pin::new(&mut rx), &mut rcv_context) {
                    Poll::Pending => {
                        assert!(model.is_empty());
                    }
                    Poll::Ready(val) => {
                        assert_eq!(model.recv(), val);
                    }
                },
            }
        }

        drop(guard);
        TestResult::passed()
    }
    QuickCheck::new()
        .tests(10_000)
        .max_tests(100_000)
        .quickcheck(inner as fn(Variant, Vec<Action>) -> TestResult);
}
