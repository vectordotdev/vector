use bytes::Bytes;
use std::{
    borrow::Cow,
    convert::{TryInto, TryFrom},
    str,
    collections::{VecDeque, vec_deque::{Iter, IntoIter}},
};
use crate::mapping::parser::{MappingParser, Rule};
use pest::{Parser, iterators::Pair};

/// Lookups are precomputed event lookup paths.
///
/// They are intended to handle user input from a configuration.
///
/// Generally, these shouldn't be created on the hot path.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Lookup<'a> {
    segments: VecDeque<Segment<'a>>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Segment<'a> {
    Field(&'a str),
    Index(usize),
}

impl<'a> Segment<'a> {
    const fn field(v: &'a str) -> Segment<'a> { Segment::Field(v) }
    const fn index(v: usize) -> Segment<'a> { Segment::Index(v) }

    #[tracing::instrument(skip(segment))]
    fn from_lookup(segment: Pair<'_, Rule>) -> crate::Result<VecDeque<Segment>> {
        let rule = segment.as_rule();
        tracing::trace!(segment = segment.as_str(), ?rule, action = %"enter");
        let mut segments = VecDeque::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                Rule::path_segment => segments.append(&mut Segment::from_path_segment(inner_segment)?),
                Rule::quoted_path_segment => {
                    segments.push_back(Segment::from_quoted_path_segment(inner_segment)?)
                },
                _ => return Err(format!("Got invalid lookup rule. Got: {:?}. Want: {:?}", inner_segment.as_rule(), [
                    Rule::path_segment,
                    Rule::quoted_path_segment
                ]).into()),
            }
        }
        tracing::trace!(segment = ?segments, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(skip(segment))]
    fn from_path_segment(segment: Pair<'_, Rule>) -> crate::Result<VecDeque<Segment>> {
        let rule = segment.as_rule();
        tracing::trace!(segment = segment.as_str(), ?rule, action = %"enter");
        let mut segments = VecDeque::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                Rule::path_field_name => {
                    tracing::trace!(segment = inner_segment.as_str(), rule = ?inner_segment.as_rule(), action = %"push");
                    segments.push_back(Segment::field(inner_segment.as_str()))
                },
                Rule::path_index => segments.push_back(Segment::from_path_index(inner_segment)?),
                _ => return Err(format!("Got invalid lookup rule. Got: {:?}. Want: {:?}", inner_segment.as_rule(), [
                    Rule::path_field_name,
                    Rule::path_index
                ]).into()),
            }
        }
        tracing::trace!(segment = ?segments, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(skip(segment))]
    fn from_path_index(segment: Pair<'_, Rule>) -> crate::Result<Segment> {
        tracing::trace!(segment = segment.as_str(), rule = ?segment.as_rule(), action = %"enter");
        let segment = segment.into_inner().next()
            .expect("Did not get pair inside path_index segment. This is an invariant. Please report it.");
        match segment.as_rule() {
            Rule::inner_path_index => {
                let index = segment.as_str().parse()?;
                tracing::trace!(segment = index, rule = ?segment.as_rule(), action = %"push");
                Ok(Segment::index(index))
            },
            _ => Err(format!("Got invalid lookup rule. Got: {:?}. Want: {:?}", segment.as_rule(), [
                Rule::inner_path_index,
            ]).into()),
        }
    }

    #[tracing::instrument(skip(segment))]
    fn from_quoted_path_segment(segment: Pair<'_, Rule>) -> crate::Result<Segment> {
        tracing::trace!(segment = segment.as_str(), rule = ?segment.as_rule(), action = %"enter");
        let segment = segment.into_inner().next()
            .expect("Did not get pair inside quoted_path_segment segment. This is an invariant. Please report it.");
        match segment.as_rule() {
            Rule::inner_quoted_string => {
                tracing::trace!(segment = segment.as_str(), rule = ?segment.as_rule(), action = %"push");
                Ok(Segment::field(segment.as_str()))
            },
            _ => return Err(format!("Got invalid lookup rule. Got: {:?}. Want: {:?}", segment.as_rule(), [
                Rule::inner_quoted_string,
            ]).into()),
        }
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
    fn into_iter(self) -> IntoIter<Segment<'a>> {
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


#[cfg(test)]
mod test {
    use super::*;

    const SUFFICIENTLY_COMPLEX: &str = r#"regular."quoted"."quoted but spaces"."quoted.but.periods".lookup[0].nested_lookup[0][0]"#;
    const SUFFICIENTLY_DECOMPOSED: [Segment; 9] = [
        Segment::field(r#"regular"#),
        Segment::field(r#"quoted"#),
        Segment::field(r#"quoted but spaces"#),
        Segment::field(r#"quoted.but.periods"#),
        Segment::field(r#"lookup"#),
        Segment::index(0),
        Segment::field(r#"nested_lookup"#),
        Segment::index(0),
        Segment::index(0),
    ];

    #[test]
    fn iter() {
        crate::test_util::trace_init();
        let lookup = Lookup::try_from(SUFFICIENTLY_COMPLEX).unwrap();

        let mut iter = lookup.iter();
        for (index, expected) in SUFFICIENTLY_DECOMPOSED.iter().enumerate() {
            let parsed = iter.next().expect(&format!("Expected at index {}: {:?}, got None.", index, expected));
            assert_eq!(
                expected,
                parsed,
                "Failed at {}", index
            );
        }
    }

    #[test]
    fn into_iter() {
        crate::test_util::trace_init();
        let lookup = Lookup::try_from(SUFFICIENTLY_COMPLEX).unwrap();
        let mut iter = lookup.into_iter();
        for (index, expected) in SUFFICIENTLY_DECOMPOSED.iter().cloned().enumerate() {
            let parsed = iter.next().expect(&format!("Expected at index {}: {:?}, got None.", index, expected));
            assert_eq!(
                expected,
                parsed,
                "Failed at {}", index
            );
        }
    }
}
