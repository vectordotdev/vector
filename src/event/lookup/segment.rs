use crate::event::lookup::SegmentBuf;
use remap::parser::ParserRule;
use pest::iterators::Pair;
use std::fmt::{Display, Formatter};

/// Segments are chunks of a lookup. They represent either a field or an index.
/// A sequence of Segments can become a lookup.
///
/// If you need an owned, allocated version, see `SegmentBuf`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
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

    #[tracing::instrument(level = "trace", skip(segment))]
    pub(crate) fn from_lookup(segment: Pair<'a, ParserRule>) -> crate::Result<Vec<Segment<'a>>> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut segments = Vec::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                ParserRule::lookup_segment => {
                    segments.append(&mut Segment::from_lookup_segment(inner_segment)?)
                },
                _ => {
                    return Err(format!(
                        "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                        inner_segment.as_rule(),
                        [ParserRule::lookup]
                    )
                        .into())
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub(crate) fn from_lookup_segment(segment: Pair<'a, ParserRule>) -> crate::Result<Vec<Segment<'a>>> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut segments = Vec::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                ParserRule::lookup_field => {
                    segments.push(Segment::from_lookup_field(inner_segment)?)
                },
                ParserRule::lookup_field_quoted => {
                    segments.push(Segment::from_lookup_field_quoted(inner_segment)?)
                },
                ParserRule::lookup_array => {
                    segments.push(Segment::from_lookup_array(inner_segment)?)
                },
                _ => {
                    return Err(format!(
                        "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                        inner_segment.as_rule(),
                        [ParserRule::lookup_array, ParserRule::lookup_field],
                    )
                    .into())
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub(crate) fn from_lookup_field(segment: Pair<'a, ParserRule>) -> crate::Result<Segment<'a>> {
        let rule = segment.as_rule();
        let segment_str = segment.as_str();
        tracing::trace!(segment = %segment_str, ?rule, action = %"enter");
        let retval = match rule {
            ParserRule::lookup_field => {
                tracing::trace!(segment = %segment_str, ?rule, action = %"push");
                Segment::field(segment_str)
            },
            _ => Err(format!(
                "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                rule,
                ParserRule::lookup_field,
            ))?,
        };
        tracing::trace!(segment = %segment_str, ?rule, action = %"exit");
        Ok(retval)
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub(crate) fn from_lookup_field_quoted(segment: Pair<'a, ParserRule>) -> crate::Result<Segment<'a>> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut retval = None;
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                ParserRule::lookup_field_quoted_content => {
                    debug_assert!(retval.is_none());
                    retval = Some(Segment::field(full_segment));
                },
                ParserRule::LOOKUP_QUOTE => continue,
                _ => {
                    return Err(format!(
                        "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                        inner_segment.as_rule(),
                        [ParserRule::lookup_field_quoted_content, ParserRule::LOOKUP_QUOTE],
                    )
                        .into())
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        retval.ok_or("Expected inner lookup segment, did not get one.".into())
    }


    #[tracing::instrument(level = "trace", skip(segment))]
    pub(crate) fn from_lookup_array(segment: Pair<'a, ParserRule>) -> crate::Result<Segment<'a>> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut retval = None;
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                ParserRule::lookup_array_index => {
                    retval = Some(Segment::from_lookup_array_index(inner_segment)?);
                    break;
                },
                _ => {
                    return Err(format!(
                        "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                        inner_segment.as_rule(),
                        [ParserRule::lookup_array_index]
                    )
                        .into())
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        retval.ok_or("Expected array index, did not get one.".into())
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub(crate) fn from_lookup_array_index(segment: Pair<'a, ParserRule>) -> crate::Result<Segment<'a>> {
        let rule = segment.as_rule();
        let segment_str = segment.as_str();
        tracing::trace!(segment = %segment_str, ?rule, action = %"enter");
        let retval = match rule {
            ParserRule::lookup_array_index => {
                let index = segment.as_str().parse()?;
                tracing::trace!(segment = %index, ?rule, action = %"push");
                Segment::index(index)
            }
            _ => Err(format!(
                "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                rule,
                ParserRule::lookup_array_index,
            ))?,
        };
        tracing::trace!(segment = %segment_str, ?rule, action = %"exit");
        Ok(retval)
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
        match v {
            SegmentBuf::Field(f) => Self::Field(f),
            SegmentBuf::Index(i) => Self::Index(*i),
        }
    }
}
