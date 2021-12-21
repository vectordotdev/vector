use std::{num::NonZeroU16, path::PathBuf};

#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use tracing::Span;

use crate::{
    topology::{
        builder::TopologyBuilder,
        channel::{BufferReceiver, BufferSender},
    },
    variant::{DiskV1Buffer, DiskV2Buffer, MemoryV1Buffer, MemoryV2Buffer},
    Bufferable, WhenFull,
};

#[cfg(test)]
const MAX_STR_SIZE: usize = 128;
#[cfg(test)]
const ALPHABET: [&str; 27] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z", "_",
];

// Memory buffers are also used with transforms which is opaque to users. We use
// an instrument flag in the Memory variant to disable instrumentation to avoid
// emitting metrics for such buffers.
#[derive(Debug, Clone)]
pub enum Variant {
    MemoryV1 {
        max_events: usize,
        when_full: WhenFull,
    },
    MemoryV2 {
        max_events: usize,
        when_full: WhenFull,
    },
    DiskV1 {
        max_size: u64,
        when_full: WhenFull,
        data_dir: PathBuf,
        id: String,
    },
    DiskV2 {
        max_size: u64,
        when_full: WhenFull,
        data_dir: PathBuf,
        id: String,
    },
}

impl Variant {
    pub async fn create_sender_receiver<T>(&self) -> (BufferSender<T>, BufferReceiver<T>)
    where
        T: Bufferable + Clone,
    {
        let mut builder = TopologyBuilder::default();
        match self {
            Variant::MemoryV1 {
                max_events,
                when_full,
                ..
            } => {
                builder.stage(MemoryV1Buffer::new(*max_events), *when_full);
            }
            Variant::MemoryV2 {
                max_events,
                when_full,
                ..
            } => {
                builder.stage(MemoryV2Buffer::new(*max_events), *when_full);
            }
            Variant::DiskV1 {
                max_size,
                when_full,
                data_dir,
                id,
            } => {
                builder.stage(
                    DiskV1Buffer::new(id.clone(), data_dir.clone(), *max_size),
                    *when_full,
                );
            }
            Variant::DiskV2 {
                max_size,
                when_full,
                data_dir,
                id,
            } => {
                builder.stage(
                    DiskV2Buffer::new(id.clone(), data_dir.clone(), *max_size),
                    *when_full,
                );
            }
        };

        let (sender, receiver, _acker) = builder
            .build(Span::none())
            .await
            .expect("topology build should not fail");

        (sender, receiver)
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct Id {
    inner: String,
}

#[cfg(test)]
impl Arbitrary for Id {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut id = String::with_capacity(MAX_STR_SIZE);
        for _ in 0..(g.size() % MAX_STR_SIZE) {
            let idx: usize = usize::arbitrary(g) % ALPHABET.len();
            id.push_str(ALPHABET[idx]);
        }

        Id { inner: id }
    }
}

#[cfg(test)]
impl Arbitrary for Variant {
    fn arbitrary(g: &mut Gen) -> Self {
        let idx = usize::arbitrary(g) % 4;

        // Using a u16 ensures we avoid any allocation errors for our holding buffers, etc.
        let max_events = NonZeroU16::arbitrary(g)
            .get()
            .try_into()
            .expect("we don't support 16-bit platforms");
        let max_size = u64::from(NonZeroU16::arbitrary(g).get());
        let when_full = WhenFull::arbitrary(g);

        match idx {
            0 => Variant::MemoryV1 {
                max_events,
                when_full,
            },
            1 => Variant::MemoryV2 {
                max_events,
                when_full,
            },
            2 => Variant::DiskV1 {
                max_size,
                when_full,
                id: Id::arbitrary(g).inner,
                data_dir: PathBuf::arbitrary(g),
            },
            3 => Variant::DiskV2 {
                max_size,
                when_full,
                id: Id::arbitrary(g).inner,
                data_dir: PathBuf::arbitrary(g),
            },
            _ => unreachable!("idx divisor should be 4"),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            Variant::MemoryV1 {
                max_events,
                when_full,
                ..
            } => {
                let when_full = *when_full;
                Box::new(max_events.shrink().map(move |me| Variant::MemoryV1 {
                    max_events: me,
                    when_full,
                }))
            }
            Variant::MemoryV2 {
                max_events,
                when_full,
                ..
            } => {
                let when_full = *when_full;
                Box::new(max_events.shrink().map(move |me| Variant::MemoryV2 {
                    max_events: me,
                    when_full,
                }))
            }
            Variant::DiskV1 {
                max_size,
                when_full,
                id,
                data_dir,
                ..
            } => {
                let max_size = *max_size;
                let when_full = *when_full;
                let id = id.clone();
                let data_dir = data_dir.clone();
                Box::new(max_size.shrink().map(move |ms| Variant::DiskV1 {
                    max_size: ms,
                    when_full,
                    id: id.clone(),
                    data_dir: data_dir.clone(),
                }))
            }
            Variant::DiskV2 {
                max_size,
                when_full,
                id,
                data_dir,
                ..
            } => {
                let max_size = *max_size;
                let when_full = *when_full;
                let id = id.clone();
                let data_dir = data_dir.clone();
                Box::new(max_size.shrink().map(move |ms| Variant::DiskV2 {
                    max_size: ms,
                    when_full,
                    id: id.clone(),
                    data_dir: data_dir.clone(),
                }))
            }
        }
    }
}
