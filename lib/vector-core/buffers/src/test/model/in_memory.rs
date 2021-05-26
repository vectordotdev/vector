use crate::test::model::{Message, Model};
use crate::{Variant, WhenFull};
use std::collections::VecDeque;

/// `InMemory` is the `Model` for on-disk buffer
pub(crate) struct InMemory {
    inner: VecDeque<Message>,
    when_full: WhenFull,
    num_senders: usize,
    capacity: usize,
}

impl InMemory {
    pub(crate) fn new(variant: &Variant, num_senders: usize) -> Self {
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
                if self.inner.len() == (self.capacity + self.num_senders) {
                    // DropNewest never blocks, instead it silently drops the
                    // item pushed in when the buffer is too full.
                } else {
                    self.inner.push_back(item);
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
