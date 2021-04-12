use crate::*;
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
    Index(isize),
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
        matches!(
            self,
            Segment::Field {
                name: _,
                requires_quoting: _
            }
        )
    }

    pub const fn index(v: isize) -> Segment<'a> {
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

impl<'a> From<isize> for Segment<'a> {
    fn from(value: isize) -> Self {
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
