use crate::event::lookup::Segment;
use std::fmt::{Display, Formatter};

/// `SegmentBuf`s are chunks of a `LookupBuf`.
///
/// They represent either a field or an index. A sequence of `SegmentBuf`s can become a `LookupBuf`.
///
/// This is the owned, allocated side of a `Segement` for `LookupBuf.` It owns its fields unlike `Lookup`. Think of `String` to `&str` or `PathBuf` to `Path`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub enum SegmentBuf {
    Field(String),
    Index(usize),
}

impl SegmentBuf {
    pub const fn field(v: String) -> SegmentBuf {
        SegmentBuf::Field(v)
    }

    pub fn is_field(&self) -> bool {
        matches!(self, SegmentBuf::Field(_))
    }

    pub const fn index(v: usize) -> SegmentBuf {
        SegmentBuf::Index(v)
    }

    pub fn is_index(&self) -> bool {
        matches!(self, SegmentBuf::Index(_))
    }

    #[instrument]
    pub(crate) fn as_segment(&self) -> Segment<'_> {
        match self {
            SegmentBuf::Field(f) => Segment::field(f.as_ref()),
            SegmentBuf::Index(i) => Segment::index(*i),
        }
    }
}

impl Display for SegmentBuf {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            SegmentBuf::Index(i) => write!(formatter, "{}", i),
            SegmentBuf::Field(f) => write!(formatter, "{}", f),
        }
    }
}

impl From<String> for SegmentBuf {
    fn from(s: String) -> Self {
        Self::Field(s)
    }
}

impl From<usize> for SegmentBuf {
    fn from(u: usize) -> Self {
        Self::index(u)
    }
}

impl<'a> From<Segment<'a>> for SegmentBuf {
    fn from(value: Segment<'a>) -> Self {
        value.as_segment_buf()
    }
}
