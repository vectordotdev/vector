use crate::WhenFull;
#[cfg(test)]
use quickcheck::{Arbitrary, Gen};

#[derive(Debug, Clone)]
pub enum Variant {
    Memory {
        max_events: usize,
        when_full: WhenFull,
    },
}

#[cfg(test)]
impl Arbitrary for Variant {
    fn arbitrary(g: &mut Gen) -> Self {
        Variant::Memory {
            max_events: u16::arbitrary(g) as usize, // u16 avoids allocation failures
            when_full: WhenFull::arbitrary(g),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match *self {
            Variant::Memory {
                max_events,
                when_full,
            } => Box::new(max_events.shrink().map(move |me| Variant::Memory {
                max_events: me,
                when_full,
            })),
        }
    }
}
