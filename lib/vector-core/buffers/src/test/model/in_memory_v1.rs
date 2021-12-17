use std::collections::VecDeque;

use super::Progress;
use crate::{
    test::{
        common::Variant,
        model::{Message, Model},
    },
    WhenFull,
};

/// `InMemory` is the `Model` for in-memory v1 buffers, based on `futures`.
pub(crate) struct InMemoryV1 {
    inner: VecDeque<Message>,
    when_full: WhenFull,
    num_senders: usize,
    capacity: usize,
}

impl InMemoryV1 {
    pub(crate) fn new(variant: &Variant, num_senders: usize) -> Self {
        match variant {
            Variant::MemoryV1 {
                max_events,
                when_full,
                ..
            } => InMemoryV1 {
                inner: VecDeque::with_capacity(*max_events),
                capacity: *max_events,
                num_senders,
                when_full: *when_full,
            },
            _ => unreachable!(),
        }
    }
}

impl Model for InMemoryV1 {
    fn send(&mut self, item: Message) -> Progress {
        match self.when_full {
            WhenFull::DropNewest => {
                if self.inner.len() >= (self.capacity + self.num_senders) {
                    // DropNewest never blocks, instead it silently drops the
                    // item pushed in when the buffer is too full.
                } else {
                    self.inner.push_back(item);
                }
                Progress::Advanced
            }
            WhenFull::Block | WhenFull::Overflow => {
                if self.inner.len() >= (self.capacity + self.num_senders) {
                    Progress::Blocked(item)
                } else {
                    self.inner.push_back(item);
                    Progress::Advanced
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
