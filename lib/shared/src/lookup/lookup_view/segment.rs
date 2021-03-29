use crate::lookup::*;
use pest::iterators::Pair;
//use remap_lang::parser::ParserRule;
use std::{
    collections::VecDeque,
    fmt::{Display, Formatter},
};
use tracing::instrument;

/// Segments are chunks of a lookup. They represent either a field or an index.
/// A sequence of Segments can become a lookup.
///
/// If you need an owned, allocated version, see `SegmentBuf`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub enum Segment<'a> {
    Field {
        name: &'a str,
        // This is a very lazy optimization to avoid having to scan for escapes.
        requires_quoting: bool,
    },
    Index(usize),
    // Coalesces hold multiple segment sets.
    Coalesce(
        Vec<
            // Each of these can be it's own independent lookup.
            VecDeque<Self>,
        >,
    ),
}

impl<'a> Segment<'a> {
    pub const fn field(name: &'a str, requires_quoting: bool) -> Segment<'a> {
        Segment::Field {
            name,
            requires_quoting,
        }
    }

    pub fn is_field(&self) -> bool {
        matches!(self, Segment::Field { name: _, requires_quoting: _  })
    }

    pub const fn index(v: usize) -> Segment<'a> {
        Segment::Index(v)
    }

    pub fn is_index(&self) -> bool {
        matches!(self, Segment::Index(_))
    }

    pub const fn coalesce(v: Vec<VecDeque<Self>>) -> Segment<'a> {
        Segment::Coalesce(v)
    }

    pub fn is_coalesce(&self) -> bool {
        matches!(self, Segment::Coalesce(_))
    }

    /*
    #[tracing::instrument(level = "trace", skip(segment))]
    pub fn from_lookup(
        segment: Pair<'a, ParserRule>,
    ) -> Result<VecDeque<Segment<'a>>, LookupError> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut segments = VecDeque::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                ParserRule::LOOKUP_PERIOD => continue,
                ParserRule::lookup_segment => {
                    segments.append(&mut Segment::from_lookup_segment(inner_segment)?)
                }
                _ => {
                    return Err(LookupError::WrongRule {
                        wants: &[ParserRule::lookup],
                        got: inner_segment.as_rule(),
                    })
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub fn from_lookup_segment(
        segment: Pair<'a, ParserRule>,
    ) -> Result<VecDeque<Segment<'a>>, LookupError> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut segments = VecDeque::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                ParserRule::lookup_field => {
                    segments.push_back(Segment::from_lookup_field(inner_segment)?)
                }
                ParserRule::lookup_field_quoted => {
                    segments.push_back(Segment::from_lookup_field_quoted(inner_segment)?)
                }
                ParserRule::lookup_array => {
                    segments.push_back(Segment::from_lookup_array(inner_segment)?)
                }
                ParserRule::lookup_coalesce => {
                    segments.push_back(Segment::from_lookup_coalesce(inner_segment)?)
                }
                _ => {
                    return Err(LookupError::WrongRule {
                        wants: &[
                            ParserRule::lookup_array,
                            ParserRule::lookup_field_quoted,
                            ParserRule::lookup_field,
                            ParserRule::lookup_coalesce,
                        ],
                        got: inner_segment.as_rule(),
                    })
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub fn from_lookup_coalesce(segment: Pair<'a, ParserRule>) -> Result<Segment<'a>, LookupError> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut sub_segments = Vec::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                ParserRule::lookup => sub_segments.push(Segment::from_lookup(inner_segment)?),
                _ => {
                    return Err(LookupError::WrongRule {
                        wants: &[ParserRule::lookup],
                        got: inner_segment.as_rule(),
                    })
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        Ok(Segment::Coalesce(sub_segments))
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub fn from_lookup_field(segment: Pair<'a, ParserRule>) -> Result<Segment<'a>, LookupError> {
        let rule = segment.as_rule();
        let segment_str = segment.as_str();
        tracing::trace!(segment = %segment_str, ?rule, action = %"enter");
        let retval = match rule {
            ParserRule::lookup_field => {
                tracing::trace!(segment = %segment_str, ?rule, action = %"push");
                Segment::field(segment_str, false)
            }
            _ => {
                return Err(LookupError::WrongRule {
                    wants: &[ParserRule::lookup_field],
                    got: rule,
                })
            }
        };
        tracing::trace!(segment = %segment_str, ?rule, action = %"exit");
        Ok(retval)
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub fn from_lookup_field_quoted(
        segment: Pair<'a, ParserRule>,
    ) -> Result<Segment<'a>, LookupError> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut retval = None;
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                ParserRule::lookup_field_quoted_content => {
                    debug_assert!(retval.is_none());
                    let segment_str = inner_segment.as_str();
                    tracing::trace!(segment = %segment_str, ?rule, action = %"push");
                    retval = Some(Segment::field(segment_str, true));
                }
                ParserRule::LOOKUP_QUOTE => continue,
                _ => {
                    return Err(LookupError::WrongRule {
                        wants: &[
                            ParserRule::lookup_field_quoted_content,
                            ParserRule::LOOKUP_QUOTE,
                        ],
                        got: inner_segment.as_rule(),
                    })
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        retval.ok_or(LookupError::MissingInnerSegment)
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub fn from_lookup_array(segment: Pair<'a, ParserRule>) -> Result<Segment<'a>, LookupError> {
        let rule = segment.as_rule();
        let full_segment = segment.as_str();
        tracing::trace!(segment = %full_segment, ?rule, action = %"enter");
        let mut retval = None;
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                ParserRule::lookup_array_index => {
                    debug_assert!(retval.is_none());
                    tracing::trace!(segment = %inner_segment, ?rule, action = %"push");
                    retval = Some(Segment::from_lookup_array_index(inner_segment)?);
                }
                ParserRule::LOOKUP_OPEN_BRACKET | ParserRule::LOOKUP_CLOSE_BRACKET => continue,
                _ => {
                    return Err(LookupError::WrongRule {
                        wants: &[ParserRule::lookup_array_index],
                        got: inner_segment.as_rule(),
                    })
                }
            }
        }
        tracing::trace!(segment = %full_segment, ?rule, action = %"exit");
        retval.ok_or(LookupError::MissingIndex)
    }

    #[tracing::instrument(level = "trace", skip(segment))]
    pub fn from_lookup_array_index(
        segment: Pair<'a, ParserRule>,
    ) -> Result<Segment<'a>, LookupError> {
        let rule = segment.as_rule();
        let segment_str = segment.as_str();
        tracing::trace!(segment = %segment_str, ?rule, action = %"enter");
        let retval = match rule {
            ParserRule::lookup_array_index => {
                let index = segment
                    .as_str()
                    .parse()
                    .map_err(|source| LookupError::IndexParsing { source })?;
                tracing::trace!(segment = %index, ?rule, action = %"push");
                Segment::index(index)
            }
            _ => {
                return Err(LookupError::WrongRule {
                    wants: &[ParserRule::lookup_array_index],
                    got: rule,
                })
            }
        };
        tracing::trace!(segment = %segment_str, ?rule, action = %"exit");
        Ok(retval)
    }
    */

    #[instrument]
    pub fn as_segment_buf(&self) -> SegmentBuf {
        match self {
            Segment::Field {
                name,
                requires_quoting,
            } => SegmentBuf::field(name.to_string(), *requires_quoting),
            Segment::Index(i) => SegmentBuf::index(*i),
            Segment::Coalesce(v) => SegmentBuf::coalesce(
                v.iter()
                    .map(|inner| {
                        inner
                            .iter()
                            .map(|v| v.as_segment_buf())
                            .collect::<VecDeque<_>>()
                    })
                    .collect(),
            ),
        }
    }

    /// Become a `SegmentBuf` (by allocating).
    #[instrument(level = "trace")]
    pub fn into_buf(self) -> SegmentBuf {
        SegmentBuf::from(self)
    }
}

impl<'a> Display for Segment<'a> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Segment::Index(i) => write!(formatter, "{}", i),
            Segment::Field {
                name,
                requires_quoting: false,
            } => write!(formatter, "{}", name),
            Segment::Field {
                name,
                requires_quoting: true,
            } => write!(formatter, "\"{}\"", name),
            Segment::Coalesce(v) => write!(
                formatter,
                "({})",
                v.iter()
                    .map(|inner| inner
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join("."))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        }
    }
}

impl<'a> From<&'a str> for Segment<'a> {
    fn from(mut name: &'a str) -> Self {
        let requires_quoting = name.starts_with('\"');
        if requires_quoting {
            let len = name.len();
            name = &name[1..len - 1];
        }
        Self::Field {
            name,
            requires_quoting,
        }
    }
}

impl<'a> From<usize> for Segment<'a> {
    fn from(value: usize) -> Self {
        Self::index(value)
    }
}

impl<'a> From<Vec<VecDeque<Segment<'a>>>> for Segment<'a> {
    fn from(value: Vec<VecDeque<Segment<'a>>>) -> Self {
        Self::coalesce(value)
    }
}

// While testing, it can be very convienent to use the `vec![]` macro.
// This would be slow in hot release code, so we don't allow it in non-test code.
#[cfg(test)]
impl<'a> From<Vec<Vec<Segment<'a>>>> for Segment<'a> {
    fn from(value: Vec<Vec<Segment<'a>>>) -> Self {
        Self::coalesce(value.into_iter().map(|v| v.into()).collect())
    }
}

impl<'a> From<&'a SegmentBuf> for Segment<'a> {
    fn from(v: &'a SegmentBuf) -> Self {
        match v {
            SegmentBuf::Field {
                name,
                requires_quoting,
            } => Self::field(name, *requires_quoting),
            SegmentBuf::Index(i) => Self::index(*i),
            SegmentBuf::Coalesce(v) => Self::coalesce(
                v.iter()
                    .map(|inner| inner.iter().map(Into::into).collect())
                    .collect(),
            ),
        }
    }
}
