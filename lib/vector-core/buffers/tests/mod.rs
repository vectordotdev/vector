mod common;

use buffers::{self, Variant, WhenFull};
use common::Message;
use futures::task::{noop_waker, Context, Poll};
use futures::{Sink, Stream};
use quickcheck::{single_shrinker, Arbitrary, Gen, QuickCheck, TestResult};
use std::{collections::VecDeque, pin::Pin};

#[derive(Debug, Clone)]
enum Action {
    Send(u64),
    Recv,
}

impl Arbitrary for Action {
    fn arbitrary(g: &mut Gen) -> Self {
        if bool::arbitrary(g) {
            Action::Send(u64::arbitrary(g))
        } else {
            Action::Recv
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            Action::Send(val) => Box::new(val.shrink().map(|x| Action::Send(x))),
            Action::Recv => single_shrinker(Action::Recv),
        }
    }
}

struct InMemoryModel {
    inner: VecDeque<Message>,
    when_full: WhenFull,
    num_senders: usize,
    capacity: usize,
}

impl InMemoryModel {
    fn new(capacity: usize, num_senders: usize, when_full: WhenFull) -> Self {
        InMemoryModel {
            inner: VecDeque::with_capacity(capacity),
            capacity,
            num_senders,
            when_full,
        }
    }

    fn send(&mut self, item: Message) {
        match self.when_full {
            WhenFull::DropNewest => {
                if self.inner.len() != (self.capacity + self.num_senders) {
                    self.inner.push_back(item);
                }
            }
            WhenFull::Block => unimplemented!(),
        }
    }

    fn recv(&mut self) -> Option<Message> {
        match self.when_full {
            WhenFull::DropNewest => self.inner.pop_front(),
            WhenFull::Block => unimplemented!(),
        }
    }

    fn is_full(&self) -> bool {
        self.inner.len() >= self.capacity
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[test]
fn in_memory_model_dropnewest() {
    fn inner(max_events: u16, actions: Vec<Action>) -> TestResult {
        let mut model = InMemoryModel::new(max_events as usize, 1, WhenFull::DropNewest);

        let rcv_waker = noop_waker();
        let mut rcv_context = Context::from_waker(&rcv_waker);

        let snd_waker = noop_waker();
        let mut snd_context = Context::from_waker(&snd_waker);

        let variant = Variant::Memory {
            max_events: max_events as usize,
            when_full: WhenFull::DropNewest,
        };
        let (tx, mut rx, _) = buffers::build::<Message>(variant).unwrap();

        let mut tx = tx.get();
        let sink = tx.as_mut();

        for action in actions.into_iter() {
            match action {
                Action::Send(id) => {
                    let msg = Message::new(id);
                    match Sink::poll_ready(Pin::new(sink), &mut snd_context) {
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
                    }
                }
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
        TestResult::passed()
    }
    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(u16, Vec<Action>) -> TestResult);
}
