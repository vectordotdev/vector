use std::collections::VecDeque;

use super::Progress;
use crate::{
    encoding::Encodable,
    test::{
        common::Variant,
        model::{Message, Model},
    },
    WhenFull,
};

/// `OnDiskV2` is the `Model` for on-disk buffer for the pure Rust implementation (v2)
pub(crate) struct OnDiskV2 {
    inner: VecDeque<Message>,
    when_full: WhenFull,
    current_bytes: usize,
    capacity: usize,
}

impl OnDiskV2 {
    pub(crate) fn new(variant: &Variant) -> Self {
        match variant {
            Variant::DiskV2 {
                max_size,
                when_full,
                ..
            } => OnDiskV2 {
                inner: VecDeque::with_capacity((*max_size).try_into().unwrap_or(usize::MAX)),
                current_bytes: 0,
                capacity: (*max_size).try_into().unwrap_or(usize::MAX),
                when_full: *when_full,
            },
            _ => unreachable!(),
        }
    }
}

impl Model for OnDiskV2 {
    fn send(&mut self, item: Message) -> Progress {
        let byte_size = Encodable::encoded_size(&item).unwrap();
        match self.when_full {
            WhenFull::DropNewest => {
                if self.is_full() {
                    // DropNewest never blocks, instead it silently drops the
                    // item pushed in when the buffer is too full.
                } else {
                    self.current_bytes += byte_size;
                    self.inner.push_back(item);
                }
                Progress::Advanced
            }
            WhenFull::Block | WhenFull::Overflow => {
                if self.is_full() {
                    Progress::Blocked(item)
                } else {
                    self.current_bytes += byte_size;
                    self.inner.push_back(item);
                    Progress::Advanced
                }
            }
        }
    }

    fn recv(&mut self) -> Option<Message> {
        self.inner.pop_front().map(|msg| {
            let byte_size = Encodable::encoded_size(&msg).unwrap();
            self.current_bytes -= byte_size;
            msg
        })
    }

    fn is_full(&self) -> bool {
        self.current_bytes >= self.capacity
    }

    fn is_empty(&self) -> bool {
        self.current_bytes == 0
    }
}
