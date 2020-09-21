mod segment;
#[cfg(test)]
mod test;


use std::{
    convert::{TryFrom},
    str,
    ops::{RangeFull, RangeToInclusive, RangeTo, RangeFrom},
    slice::Iter,
    vec::IntoIter,
};
use crate::mapping::parser::{MappingParser, Rule};
use pest::{Parser, iterators::Pair};

pub use segment::Segment;
use std::slice::SliceIndex;
use std::ops::{Index, IndexMut};

/// Lookups are precomputed event lookup paths.
///
/// They are intended to handle user input from a configuration.
///
/// Generally, these shouldn't be created on the hot path.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Lookup<'a> {
    segments: Vec<Segment<'a>>,
}

impl<'a> AsRef<[Segment<'a>]> for Lookup<'a> {
    fn as_ref(&self) -> &[Segment<'a>] {
        self.segments.as_ref()
    }
}

impl<'a> TryFrom<Pair<'a, Rule>> for Lookup<'a> {
    type Error = crate::Error;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        Ok(Self { segments: Segment::from_lookup(pair)? })
    }
}

impl<'a> Lookup<'a> {
    fn iter(&self) -> Iter<'_, Segment<'_>> {
        self.segments.iter()
    }
}


impl<'a> IntoIterator for Lookup<'a> {
    type Item = Segment<'a>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments.into_iter()
    }
}

impl<'a> TryFrom<&'a str> for Lookup<'a> {
    type Error = crate::Error;

    fn try_from(input: &'a str) -> Result<Self, Self::Error> {
        let mut pairs = MappingParser::parse(Rule::lookup, input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Self::try_from(pair)
    }
}

impl<'a> Index<usize> for Lookup<'a> {
    type Output = Segment<'a>;

    fn index(&self, index: usize) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> Index<RangeFull> for Lookup<'a> {
    type Output = [Segment<'a>];

    fn index(&self, index: RangeFull) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> Index<RangeToInclusive<usize>> for Lookup<'a> {
    type Output = [Segment<'a>];

    fn index(&self, index: RangeToInclusive<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> Index<RangeTo<usize>> for Lookup<'a> {
    type Output = [Segment<'a>];

    fn index(&self, index: RangeTo<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> Index<RangeFrom<usize>> for Lookup<'a> {
    type Output = [Segment<'a>];

    fn index(&self, index: RangeFrom<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> IndexMut<usize> for Lookup<'a> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.segments.index_mut(index)
    }
}