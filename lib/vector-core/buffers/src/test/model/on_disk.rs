use crate::test::model::{Message, Model};
use crate::{EncodeBytes, Variant, WhenFull};
use std::collections::VecDeque;

use super::Progress;

/// `OnDisk` is the `Model` for on-disk buffer
#[cfg(feature = "disk-buffer")]
pub(crate) struct OnDisk {
    inner: VecDeque<Message>,
    when_full: WhenFull,
    current_bytes: usize,
    capacity: usize,
}

#[cfg(feature = "disk-buffer")]
impl OnDisk {
    pub(crate) fn new(variant: &Variant) -> Self {
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

#[cfg(feature = "disk-buffer")]
impl Model for OnDisk {
    fn send(&mut self, item: Message) -> Progress {
        let byte_size = EncodeBytes::encoded_size(&item).unwrap();
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
            let byte_size = EncodeBytes::encoded_size(&msg).unwrap();
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
