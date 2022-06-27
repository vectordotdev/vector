use std::collections::VecDeque;

use super::Progress;
use crate::{
    test::{
        common::Variant,
        model::{Message, Model},
    },
    WhenFull,
};

/// `InMemory` is the `Model` for in-memory v2 buffers, based on `tokio`.
pub(crate) struct InMemory {
    inner: VecDeque<Message>,
    when_full: WhenFull,
    capacity: usize,
}

impl InMemory {
    pub(crate) fn new(variant: &Variant) -> Self {
        match variant {
            Variant::Memory {
                max_events,
                when_full,
                ..
            } => InMemory {
                inner: VecDeque::with_capacity(max_events.get()),
                capacity: max_events.get(),
                when_full: *when_full,
            },
            _ => unreachable!(),
        }
    }
}

impl Model for InMemory {
    fn send(&mut self, item: Message) -> Progress {
        match self.when_full {
            WhenFull::DropNewest => {
                if self.inner.len() >= self.capacity {
                    // DropNewest never blocks, instead it silently drops the
                    // item pushed in when the buffer is too full.
                } else {
                    self.inner.push_back(item);
                }
                Progress::Advanced
            }
            WhenFull::Block | WhenFull::Overflow => {
                if self.inner.len() >= self.capacity {
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
