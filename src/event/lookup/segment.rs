use crate::mapping::parser::Rule;
use pest::iterators::Pair;
use std::fmt::{Display, Formatter};
use crate::event::lookup::SegmentBuf;

/// Segments are chunks of a lookup. They represent either a field or an index.
/// A sequence of Segments can become a lookup.
///
/// If you need an owned, allocated version, see `SegmentBuf`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub enum Segment<'a> {
    Field(&'a str),
    Index(usize),
}

impl<'a> Segment<'a> {
    pub const fn field(v: &'a str) -> Segment<'a> {
        Segment::Field(v)
    }

    pub fn is_field(&self) -> bool {
        matches!(self, Segment::Field(_))
    }

    pub const fn index(v: usize) -> Segment<'a> {
        Segment::Index(v)
    }

    pub fn is_value(&self) -> bool {
        matches!(self, Segment::Index(_))
    }

    #[tracing::instrument(skip(segment))]
    pub(crate) fn from_lookup(segment: Pair<'a, Rule>) -> crate::Result<Vec<Segment<'a>>> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut segments = Vec::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                Rule::path_segment => {
                    segments.append(&mut Segment::from_path_segment(inner_segment)?)
                }
                Rule::quoted_path_segment => {
                    segments.push(Segment::from_quoted_path_segment(inner_segment)?)
                }
                _ => {
                    return Err(format!(
                        "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                        inner_segment.as_rule(),
                        [Rule::path_segment, Rule::quoted_path_segment]
                    )
                    .into())
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(skip(segment))]
    pub(crate) fn from_path_segment(segment: Pair<'a, Rule>) -> crate::Result<Vec<Segment<'a>>> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut segments = Vec::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                Rule::path_field_name => {
                    tracing::trace!(segment = %inner_segment.as_str(), rule = ?inner_segment.as_rule(), action = %"push");
                    segments.push(Segment::field(inner_segment.as_str()))
                }
                Rule::path_index => segments.push(Segment::from_path_index(inner_segment)?),
                _ => {
                    return Err(format!(
                        "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                        inner_segment.as_rule(),
                        [Rule::path_field_name, Rule::path_index]
                    )
                    .into())
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(skip(segment))]
    pub(crate) fn from_path_index(segment: Pair<'a, Rule>) -> crate::Result<Segment<'a>> {
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, rule = ?segment.as_rule(), action = %"enter");
        let segment = segment.into_inner().next().expect(
            "Did not get pair inside path_index segment. This is an invariant. Please report it.",
        );
        let retval = match segment.as_rule() {
            Rule::inner_path_index => {
                let index = segment.as_str().parse()?;
                tracing::trace!(segment = %index, rule = ?segment.as_rule(), action = %"push");
                Ok(Segment::index(index))
            }
            _ => Err(format!(
                "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                segment.as_rule(),
                [Rule::inner_path_index,]
            )
            .into()),
        };
        tracing::trace!(segment = %full_segment, rule = ?segment.as_rule(), action = %"exit");
        retval
    }

    #[tracing::instrument(skip(segment))]
    pub(crate) fn from_quoted_path_segment(segment: Pair<'a, Rule>) -> crate::Result<Segment<'a>> {
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, rule = ?segment.as_rule(), action = %"enter");
        let segment = segment.into_inner().next()
            .expect("Did not get pair inside quoted_path_segment segment. This is an invariant. Please report it.");
        let retval = match segment.as_rule() {
            Rule::inner_quoted_string => {
                tracing::trace!(segment = %segment.as_str(), rule = ?segment.as_rule(), action = %"push");
                Ok(Segment::field(
                    full_segment,
                ))
            }
            _ => {
                return Err(format!(
                    "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                    segment.as_rule(),
                    [Rule::inner_quoted_string,]
                )
                .into())
            }
        };
        tracing::trace!(segment = %full_segment, rule = ?segment.as_rule(), action = %"exit");
        retval
    }

    #[instrument]
    pub(crate) fn as_segment_buf(&self) -> SegmentBuf {
        match self {
            Segment::Field(f) => SegmentBuf::field(f.to_string()),
            Segment::Index(i) => SegmentBuf::index(*i),
        }
    }
}

impl<'a> Display for Segment<'a> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Segment::Index(i) => write!(formatter, "{}", i),
            Segment::Field(f) => write!(formatter, "{}", f),
        }
    }
}

impl<'a> From<&'a str> for Segment<'a> {
    fn from(s: &'a str) -> Self {
        Self::Field(s)
    }
}

impl<'a> From<usize> for Segment<'a> {
    fn from(u: usize) -> Self {
        Self::index(u)
    }
}

impl<'a> From<&'a SegmentBuf> for Segment<'a> {
    fn from(v: &'a SegmentBuf) -> Self {
        v.as_segment()
    }
}
