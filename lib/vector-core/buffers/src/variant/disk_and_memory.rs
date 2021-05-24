use crate::WhenFull;
use quickcheck::{Arbitrary, Gen};
use std::path::PathBuf;

const MAX_STR_SIZE: usize = 128;
const ALPHABET: [&str; 27] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z", "_",
];

#[derive(Debug, Clone)]
pub enum Variant {
    Memory {
        max_events: usize,
        when_full: WhenFull,
    },
    Disk {
        max_size: usize,
        when_full: WhenFull,
        data_dir: PathBuf,
        name: String,
    },
}

#[derive(Debug, Clone)]
struct Name {
    inner: String,
}

impl Arbitrary for Name {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut name = String::with_capacity(MAX_STR_SIZE);
        for _ in 0..(g.size() % MAX_STR_SIZE) {
            let idx: usize = usize::arbitrary(g) % ALPHABET.len();
            name.push_str(ALPHABET[idx]);
        }

        Name { inner: name }
    }
}

impl Arbitrary for Variant {
    fn arbitrary(g: &mut Gen) -> Self {
        if bool::arbitrary(g) {
            Variant::Memory {
                max_events: u16::arbitrary(g) as usize, // u16 avoids allocation failures
                when_full: WhenFull::arbitrary(g),
            }
        } else {
            Variant::Disk {
                max_size: u16::arbitrary(g) as usize, // u16 avoids allocation failures
                when_full: WhenFull::arbitrary(g),
                name: Name::arbitrary(g).inner,
                data_dir: PathBuf::arbitrary(g),
            }
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            Variant::Memory {
                max_events,
                when_full,
            } => {
                let when_full = when_full.clone();
                Box::new(max_events.shrink().map(move |me| Variant::Memory {
                    max_events: me,
                    when_full,
                }))
            }
            Variant::Disk {
                max_size,
                when_full,
                name,
                data_dir,
            } => {
                // Satisfying lifetimes when shrinking is tough. While there's
                // surely a more tidy way of doing this I could find nothing
                // more terse. -blt
                let max_size = max_size.clone();
                let when_full = when_full.clone();
                let name = name.clone();
                let data_dir = data_dir.clone();
                Box::new(max_size.shrink().map(move |ms| Variant::Disk {
                    max_size: ms,
                    when_full,
                    name: name.clone(),
                    data_dir: data_dir.clone(),
                }))
            }
        }
    }
}
