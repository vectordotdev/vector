use crate::WhenFull;
#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use std::path::PathBuf;

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
    Memory {
        max_events: usize,
        when_full: WhenFull,
        instrument: bool,
    },
    Disk {
        max_size: usize,
        when_full: WhenFull,
        data_dir: PathBuf,
        id: String,
    },
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
        if bool::arbitrary(g) {
            Variant::Memory {
                max_events: u16::arbitrary(g) as usize, // u16 avoids allocation failures
                when_full: WhenFull::arbitrary(g),
                instrument: false,
            }
        } else {
            Variant::Disk {
                max_size: u16::arbitrary(g) as usize, // u16 avoids allocation failures
                when_full: WhenFull::arbitrary(g),
                id: Id::arbitrary(g).inner,
                data_dir: PathBuf::arbitrary(g),
            }
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            Variant::Memory {
                max_events,
                when_full,
                instrument,
                ..
            } => {
                let when_full = *when_full;
                let instrument = *instrument;
                Box::new(max_events.shrink().map(move |me| Variant::Memory {
                    max_events: me,
                    when_full,
                    instrument,
                }))
            }
            Variant::Disk {
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
                Box::new(max_size.shrink().map(move |ms| Variant::Disk {
                    max_size: ms,
                    when_full,
                    id: id.clone(),
                    data_dir: data_dir.clone(),
                }))
            }
        }
    }
}
