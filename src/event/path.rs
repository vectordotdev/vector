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
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Lookup {
    segments: VecDeque<Segment>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Segment {
    Field(String),
    Index(usize),
}

impl Segment {
    const fn field(v: String) -> Segment { Segment::Field(v) }
    const fn index(v: usize) -> Segment { Segment::Index(v) }
}

impl<'a> From<Pair<'a, Rule>> for Lookup {
    fn from(pairs: Pair<'a, Rule>) -> Self {
        let mut segments = VecDeque::default();
        for segment in pairs.into_inner() {
            match segment.as_rule() {
                Rule::path_segment => segments.push_back(Segment::field(segment.as_str().to_string())),
                Rule::quoted_path_segment => segments.push_back(Segment::field(segment.as_str().to_string())),
                _ => panic!("At the disco {:?}", segment)
            }
        }
        Self { segments }
    }
}

impl Lookup {
    fn iter(&self) -> Iter<'_, Segment> {
        self.segments.iter()
    }
    fn into_iter(self) -> IntoIter<Segment> {
        self.segments.into_iter()
    }
}

impl TryFrom<Bytes> for Lookup {
    type Error = crate::Error;

    fn try_from(input: Bytes) -> Result<Self, Self::Error> {
        // While it's not ideal to take the bytes as a utf-8 string here, it sadly must be done.
        // It would be invalid to have a non-UTF-8 lookup anyways, so this isn't necessarily a bad thing,
        // it's just expensive. But since this is taking from bytes, we have to.
        let input = str::from_utf8(&*input)?;
        Lookup::try_from(input)
    }
}

impl TryFrom<&str> for Lookup {
    type Error = crate::Error;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        let mut pairs = MappingParser::parse(Rule::dot_path, input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Ok(Self::from(pair))
    }
}

impl TryFrom<&String> for Lookup {
    type Error = crate::Error;

    fn try_from(input: &String) -> Result<Self, Self::Error> {
        let mut pairs = MappingParser::parse(Rule::dot_path, input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Ok(Self::from(pair))
    }
}


#[cfg(test)]
mod test {
    use super::*;

    const SUFFICIENTLY_COMPLEX: &str = r#".regular."quoted".lookup[0]."escapes_the_\.".handles_the_tail"#;
    const SUFFICIENTLY_DECOMPOSED: [&str; 5] = [
        r#"regular"#,
        r#""quoted""#,
        r#"lookup[0]"#,
        r#"escapes_the_\."#,
        r#"handles_the_tail"#,
    ];

    #[test]
    fn iter() {
        let lookup = Lookup::try_from(SUFFICIENTLY_COMPLEX).unwrap();
        for (index, item) in lookup.iter().enumerate() {
            assert_eq!(
                &Segment::field(SUFFICIENTLY_DECOMPOSED[index].parse().unwrap()),
                item
            );
        }
    }

    #[test]
    fn into_iter() {
        let lookup = Lookup::try_from(SUFFICIENTLY_COMPLEX).unwrap();
        for (index, item) in lookup.into_iter().enumerate() {
            assert_eq!(
                Segment::field(SUFFICIENTLY_DECOMPOSED[index].parse().unwrap()),
                item
            );
        }
    }
}
