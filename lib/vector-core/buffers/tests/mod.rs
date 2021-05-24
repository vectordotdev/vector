mod common;

use crate::common::Action;
use buffers::{self, bytes::EncodeBytes, Variant, WhenFull};
use common::Message;
use futures::task::{noop_waker, Context, Poll};
use futures::{Sink, Stream};
use quickcheck::{QuickCheck, TestResult};
use std::{collections::VecDeque, pin::Pin};

trait Model {
    fn send(&mut self, item: Message);
    fn recv(&mut self) -> Option<Message>;
    fn is_full(&self) -> bool;
    fn is_empty(&self) -> bool;
}

struct OnDisk {
    inner: VecDeque<Message>,
    when_full: WhenFull,
    current_bytes: usize,
    capacity: usize,
}

impl OnDisk {
    fn new(variant: &Variant) -> Self {
        match variant {
            Variant::Memory { .. } => unreachable!(),
            #[cfg(feature = "disk-buffer")]
            Variant::Disk {
                max_size,
                when_full,
                ..
            } => OnDisk {
                inner: VecDeque::with_capacity(*max_size),
                current_bytes: 0,
                capacity: *max_size,
                when_full: *when_full,
            },
        }
    }
}

impl Model for OnDisk {
    fn send(&mut self, item: Message) {
        let byte_size = EncodeBytes::encoded_size(&item).unwrap();
        match self.when_full {
            WhenFull::DropNewest => {
                if !self.is_full() {
                    self.current_bytes += byte_size;
                    self.inner.push_back(item);
                } else {
                    // DropNewest never blocks, instead it silently drops the
                    // item pushed in when the buffer is too full.
                }
            }
            WhenFull::Block => {
                if !self.is_full() {
                    self.current_bytes += byte_size;
                    self.inner.push_back(item);
                }
            }
        }
    }

    fn recv(&mut self) -> Option<Message> {
        if let Some(msg) = self.inner.pop_front() {
            let byte_size = EncodeBytes::encoded_size(&msg).unwrap();
            self.current_bytes -= byte_size;
            Some(msg)
        } else {
            None
        }
    }

    fn is_full(&self) -> bool {
        self.current_bytes >= self.capacity
    }

    fn is_empty(&self) -> bool {
        self.current_bytes == 0
    }
}

struct InMemory {
    inner: VecDeque<Message>,
    when_full: WhenFull,
    num_senders: usize,
    capacity: usize,
}

impl InMemory {
    fn new(variant: &Variant, num_senders: usize) -> Self {
        match variant {
            Variant::Memory {
                max_events,
                when_full,
            } => InMemory {
                inner: VecDeque::with_capacity(*max_events),
                capacity: *max_events,
                num_senders,
                when_full: *when_full,
            },
            #[cfg(feature = "disk-buffer")]
            _ => unreachable!(),
        }
    }
}

impl Model for InMemory {
    fn send(&mut self, item: Message) {
        match self.when_full {
            WhenFull::DropNewest => {
                if self.inner.len() != (self.capacity + self.num_senders) {
                    self.inner.push_back(item);
                } else {
                    // DropNewest never blocks, instead it silently drops the
                    // item pushed in when the buffer is too full.
                }
            }
            WhenFull::Block => {
                if self.inner.len() != (self.capacity + self.num_senders) {
                    self.inner.push_back(item);
                }
            }
        }
    }

    fn recv(&mut self) -> Option<Message> {
        self.inner.pop_front()
    }

    fn is_full(&self) -> bool {
        self.inner.len() >= self.capacity
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

fn cleanup(variant: Variant) {
    match variant {
        Variant::Memory { .. } => { /* nothing to clean up */ }
        #[cfg(feature = "disk-buffer")]
        Variant::Disk { data_dir, .. } => {
            let _ = std::fs::remove_dir_all(data_dir);
        }
    }
}

#[test]
fn in_memory_model() {
    fn inner(variant: Variant, actions: Vec<Action>) -> TestResult {
        let mut model: Box<dyn Model> = match variant {
            Variant::Memory { .. } => Box::new(InMemory::new(&variant, 1)),
            #[cfg(feature = "disk-buffer")]
            Variant::Disk { .. } => Box::new(OnDisk::new(&variant)),
        };

        let rcv_waker = noop_waker();
        let mut rcv_context = Context::from_waker(&rcv_waker);

        let snd_waker = noop_waker();
        let mut snd_context = Context::from_waker(&snd_waker);

        let (tx, mut rx, _) = buffers::build::<Message>(variant.clone()).unwrap();

        let mut tx = tx.get();
        let sink = tx.as_mut();

        for action in actions.into_iter() {
            match action {
                Action::Send(msg) => match Sink::poll_ready(Pin::new(sink), &mut snd_context) {
                    Poll::Ready(Ok(())) => {
                        assert_eq!(Ok(()), Sink::start_send(Pin::new(sink), msg));
                        model.send(msg);
                        match Sink::poll_flush(Pin::new(sink), &mut snd_context) {
                            Poll::Ready(Ok(())) => {}
                            Poll::Pending => {
                                debug_assert!(model.is_empty() || model.is_full(), "{:?}", msg)
                            }
                            Poll::Ready(Err(_)) => return TestResult::failed(),
                        }
                    }
                    Poll::Pending => assert!(model.is_full()),
                    Poll::Ready(Err(_)) => return TestResult::failed(),
                },
                Action::Recv => match Stream::poll_next(Pin::new(&mut rx), &mut rcv_context) {
                    Poll::Pending => {
                        assert_eq!(true, model.is_empty())
                    }
                    Poll::Ready(val) => {
                        assert_eq!(model.recv(), val);
                    }
                },
            }
        }
        cleanup(variant);
        TestResult::passed()
    }
    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(Variant, Vec<Action>) -> TestResult);
}
