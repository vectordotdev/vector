mod segment;
#[cfg(test)]
mod test;

use crate::mapping::parser::{MappingParser, Rule};
use pest::{iterators::Pair, Parser};
use std::{
    convert::TryFrom,
    ops::{RangeFrom, RangeFull, RangeTo, RangeToInclusive},
    slice::Iter,
    str,
};

pub use segment::Segment;
use std::ops::{Index, IndexMut};

/// Lookups are precomputed event lookup paths.
///
/// They are intended to handle user input from a configuration.
///
/// Generally, these shouldn't be created on the hot path.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Lookup {
    segments: Vec<Segment>,
}

impl<'a> TryFrom<Pair<'a, Rule>> for Lookup {
    type Error = crate::Error;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        Ok(Self {
            segments: Segment::from_lookup(pair)?,
        })
    }
}

impl Lookup {
    fn iter(&self) -> Iter<'_, Segment> {
        self.segments.iter()
    }
}

impl IntoIterator for Lookup {
    type Item = Segment;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments.into_iter()
    }
}

impl TryFrom<String> for Lookup {
    type Error = crate::Error;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        let mut pairs = MappingParser::parse(Rule::lookup, &input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Self::try_from(pair)
    }
}

impl TryFrom<string_cache::DefaultAtom> for Lookup {
    type Error = crate::Error;

    fn try_from(input: string_cache::DefaultAtom) -> Result<Self, Self::Error> {
        let mut pairs = MappingParser::parse(Rule::lookup, &input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Self::try_from(pair)
    }
}

impl Index<usize> for Lookup {
    type Output = Segment;

    fn index(&self, index: usize) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeFull> for Lookup {
    type Output = [Segment];

    fn index(&self, index: RangeFull) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeToInclusive<usize>> for Lookup {
    type Output = [Segment];

    fn index(&self, index: RangeToInclusive<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeTo<usize>> for Lookup {
    type Output = [Segment];

    fn index(&self, index: RangeTo<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeFrom<usize>> for Lookup {
    type Output = [Segment];

    fn index(&self, index: RangeFrom<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl IndexMut<usize> for Lookup {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.segments.index_mut(index)
    }
}
