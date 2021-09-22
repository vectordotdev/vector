use crate::test::common::{Action, Message};
use crate::test::model::in_memory::InMemory;
#[cfg(feature = "disk-buffer")]
use crate::test::model::on_disk::OnDisk;
use crate::Variant;
use crate::WhenFull;
use futures::task::{noop_waker, Context, Poll};
use futures::{stream, SinkExt};
use futures::{Sink, Stream, StreamExt};
use proptest::prelude::*;
use std::pin::Pin;
use std::sync::Arc;
use tokio::runtime;
use tokio::sync::Barrier;

mod in_memory;
#[cfg(feature = "disk-buffer")]
mod on_disk;

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

/// `VariantGuard` wraps a `Variant`, allowing a convenient Drop implementation
struct VariantGuard {
    inner: Variant,
}

impl VariantGuard {
    fn new(variant: Variant) -> Self {
        VariantGuard { inner: variant }
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

#[cfg(feature = "disk-buffer")]
fn arb_variant() -> impl Strategy<Value = Variant> {
    prop_oneof![
        <(u16, WhenFull)>::arbitrary().prop_map(|(max_events, when_full)| {
            Variant::Memory {
                max_events: max_events as usize,
                when_full,
            }
        }),
        <(u16, WhenFull, u64)>::arbitrary().prop_map(|(max_size, when_full, id)| {
            let id = id.to_string();
            // SAFETY: We allow tempdir to create the directory but by
            // calling `into_path` we obligate ourselves to delete it. This
            // is done in the drop implementation for `VariantGuard`.
            let data_dir = tempdir::TempDir::new_in(std::env::temp_dir(), &id)
                .unwrap()
                .into_path();
            Variant::Disk {
                max_size: max_size as usize,
                when_full,
                data_dir,
                id,
            }
        })
    ]
}

#[cfg(not(feature = "disk-buffer"))]
fn arb_variant() -> impl Strategy<Value = Variant> {
    prop_oneof![
        <(u16, WhenFull)>::arbitrary().prop_map(|(max_events, when_full)| {
            Variant::Memory {
                max_events: max_events as usize,
                when_full,
            }
        }),
    ]
}

// NOTE this test will hang on the call to `rx.next()` for unknown reasons. The
// behavior here is of a sending side that does not properly hang up, leaving
// the receiving side waiting for more messages that won't come.
#[test]
fn crazy_pills_or_sender_never_hangs_up() {
    let messages = vec![Message::new(0)];
    let variant = Variant::Memory {
        max_events: 10,
        when_full: WhenFull::Block,
    };
    let guard = VariantGuard::new(variant);

    let (tx, mut rx, _) = crate::build::<Message>(guard.as_ref().clone()).unwrap();
    let runtime = runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .unwrap();

    let mut tx = tx.get();

    runtime.block_on(async move {
        let barrier = Arc::new(Barrier::new(2));

        let snd_barrier = Arc::clone(&barrier);
        let _ = tokio::spawn(async move {
            let _ = snd_barrier.wait().await;
            let mut stream = stream::iter(messages.clone()).map(|m| Ok(m));
            tx.send_all(&mut stream).await.unwrap();
            tx.close().await.unwrap();
            println!("I AM DONE");
        });

        barrier.wait().await;

        let mut base_id = 0;
        while let Some(msg) = rx.next().await {
            println!("LOOP HERE");
            assert!(base_id <= msg.id());
            base_id = msg.id();
        }

        println!("GOT HERE");
    })
}

proptest! {
    /// This test models a single sender and a single receiver pushing and
    /// pulling from a common buffer. The buffer itself may be either memory or
    /// disk. We use the raw `futures::sink::Sink` and `futures::stream::Stream`
    /// interface, avoiding the need to model the runtime in any way. This is,
    /// then, the buffer as a runtime will see it.
    ///
    /// Acks are not modeled yet. I believe doing so would be a straightforward
    /// process.
    #[test]
    fn model_check(variant in arb_variant(),
                   actions in Vec::<Action>::arbitrary()) {
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

        let (tx, rx, _) = crate::build::<Message>(guard.as_ref().clone()).unwrap();

        let mut tx = tx.get();
        let mut recv = Pin::new(rx);
        let mut sink = Pin::new(tx.as_mut());

        let runtime = runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()?;

        runtime.block_on(async {
        for action in actions {
            match action {
                // For each send action we attempt to send into the buffer and
                // if the buffer signals itself ready do the send, then
                // flush. We might profitably model a distinct flush action but
                // at the time this model was created there was no clear reason
                // to do so.
                Action::Send(msg) => match Sink::poll_ready(sink.as_mut(), &mut snd_context) {
                    Poll::Ready(Ok(())) => {
                        // Once the buffer signals its ready we are allowed to
                        // call `start_send`. The buffer may or may not make the
                        // value immediately available to a receiver, something
                        // we elide by immediately flushing.
                        assert_eq!(Ok(()), Sink::start_send(sink.as_mut(), msg.clone()));
                        assert!(matches!(model.send(msg.clone()), Progress::Advanced));
                        match Sink::poll_flush(sink.as_mut(), &mut snd_context) {
                            Poll::Ready(Ok(())) => {}
                            // If the buffer signals Ready/Ok then we're good to
                            // go. Both the model and the SUT will have received
                            // their item. However, if the SUT signals Pending
                            // then this is only valid so long as the model is
                            // full.
                            Poll::Pending => {
                                assert!(model.is_full(), "{:?}", msg);
                            }
                            // The SUT must never signal an error when we
                            // flush. There is no way to recover from an error.
                            Poll::Ready(Err(_)) => unreachable!(),
                        }
                    }
                    Poll::Pending => assert!(model.is_full()),
                    Poll::Ready(Err(_)) => unreachable!(),
                },
                Action::Recv => match Stream::poll_next(recv.as_mut(), &mut rcv_context) {
                    Poll::Pending => {
                        assert!(model.is_empty());
                    }
                    Poll::Ready(val) => {
                        assert_eq!(model.recv(), val);
                    }
                },
            }
        }
        });

        drop(guard);
    }
}
