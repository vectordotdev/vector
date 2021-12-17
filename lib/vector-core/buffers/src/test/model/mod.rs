mod in_memory_v1;
mod in_memory_v2;
mod on_disk_v1;
mod on_disk_v2;

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{task::noop_waker, Sink, Stream};
use quickcheck::{QuickCheck, TestResult};
use tokio::runtime::Runtime;

use super::common::Variant;
use crate::test::{
    common::{Action, Message},
    model::{
        in_memory_v1::InMemoryV1, in_memory_v2::InMemoryV2, on_disk_v1::OnDiskV1,
        on_disk_v2::OnDiskV2,
    },
};

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
        Variant::MemoryV1 { .. } | Variant::MemoryV2 { .. } => true,
        Variant::DiskV1 { id, data_dir, .. } | Variant::DiskV2 { id, data_dir, .. } => {
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
            Variant::MemoryV1 { .. } | Variant::MemoryV2 { .. } => VariantGuard { inner: variant },
            Variant::DiskV1 {
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
                    inner: Variant::DiskV1 {
                        max_size,
                        when_full,
                        data_dir,
                        id,
                    },
                }
            }
            Variant::DiskV2 {
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
                    inner: Variant::DiskV2 {
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
            Variant::MemoryV1 { .. } | Variant::MemoryV2 { .. } => {}
            Variant::DiskV1 { data_dir, .. } | Variant::DiskV2 { data_dir, .. } => {
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
            Variant::MemoryV1 { .. } => Box::new(InMemoryV1::new(guard.as_ref(), 1)),
            Variant::MemoryV2 { .. } => Box::new(InMemoryV2::new(guard.as_ref())),
            Variant::DiskV1 { .. } => Box::new(OnDiskV1::new(guard.as_ref())),
            Variant::DiskV2 { .. } => Box::new(OnDiskV2::new(guard.as_ref())),
        };

        let runtime = Runtime::new().unwrap();
        let (mut tx, mut rx, guard) = runtime.block_on(async move {
            let (tx, rx) = guard.as_ref().create_sender_receiver().await;
            (tx, rx, guard)
        });

        let noop_send_waker = noop_waker();
        let mut send_context = Context::from_waker(&noop_send_waker);
        let noop_recv_waker = noop_waker();
        let mut recv_context = Context::from_waker(&noop_recv_waker);

        for action in actions {
            match action {
                // For each send action we attempt to send into the buffer and
                // if the buffer signals itself ready do the send, then
                // flush. We might profitably model a distinct flush action but
                // at the time this model was created there was no clear reason
                // to do so.
                Action::Send(msg) => match Sink::poll_ready(Pin::new(&mut tx), &mut send_context) {
                    Poll::Ready(Ok(())) => {
                        // Once the buffer signals its ready we are allowed to
                        // call `start_send`. The buffer may or may not make the
                        // value immediately available to a receiver, something
                        // we elide by immediately flushing.
                        let start_send_result = Sink::start_send(Pin::new(&mut tx), msg.clone());
                        assert!(start_send_result.is_ok());
                        assert!(matches!(model.send(msg.clone()), Progress::Advanced));
                        match Sink::poll_flush(Pin::new(&mut tx), &mut send_context) {
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
                Action::Recv => match Stream::poll_next(Pin::new(&mut rx), &mut recv_context) {
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
