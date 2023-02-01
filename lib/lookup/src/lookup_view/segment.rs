use std::fmt::{Display, Formatter};

use inherent::inherent;

use crate::{field, FieldBuf, LookSegment, SegmentBuf};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Field<'a> {
    pub name: &'a str,
    // This is a very lazy optimization to avoid having to scan for escapes.
    pub requires_quoting: bool,
}

impl<'a> Field<'a> {
    pub fn as_field_buf(&self) -> FieldBuf {
        FieldBuf {
            name: self.name.to_string(),
            requires_quoting: self.requires_quoting,
        }
    }
}

impl<'a> Display for Field<'a> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        if self.requires_quoting {
            write!(formatter, r#""{}""#, self.name)
        } else {
            write!(formatter, r#"{}"#, self.name)
        }
    }
}

impl<'a> From<&'a str> for Field<'a> {
    fn from(mut name: &'a str) -> Self {
        let mut requires_quoting = false;

        if name.starts_with('\"') && name.ends_with('\"') {
            let len = name.len();
            name = &name[1..len - 1];
            requires_quoting = true;
        } else if !field::is_valid_fieldname(name) {
            requires_quoting = true;
        }

        Self {
            name,
            requires_quoting,
        }
    }
}

impl<'a> From<&'a FieldBuf> for Field<'a> {
    fn from(v: &'a FieldBuf) -> Self {
        Self {
            name: &v.name,
            requires_quoting: v.requires_quoting,
        }
    }
}

/// Segments are chunks of a lookup. They represent either a field or an index.
/// A sequence of Segments can become a lookup.
///
/// If you need an owned, allocated version, see `SegmentBuf`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub enum Segment<'a> {
    Field(Field<'a>),
    Index(isize),
    // Coalesces hold multiple possible fields.
    Coalesce(Vec<Field<'a>>),
}

impl<'a> Segment<'a> {
    pub fn as_segment_buf(&self) -> SegmentBuf {
        match self {
            Segment::Field(field) => SegmentBuf::field(field.as_field_buf()),
            Segment::Index(i) => SegmentBuf::index(*i),
            Segment::Coalesce(v) => {
                SegmentBuf::coalesce(v.iter().map(|field| field.as_field_buf()).collect())
            }
        }
    }

    /// Become a `SegmentBuf` (by allocating).
    pub fn into_buf(self) -> SegmentBuf {
        SegmentBuf::from(self)
    }
}

#[inherent]
impl<'a> LookSegment<'a> for Segment<'a> {
    type Field = Field<'a>;

    pub fn field(field: Field<'a>) -> Segment<'a> {
        Segment::Field(field)
    }

    pub fn is_field(&self) -> bool {
        matches!(self, Segment::Field(_))
    }

    pub fn index(v: isize) -> Segment<'a> {
        Segment::Index(v)
    }

    pub fn is_index(&self) -> bool {
        matches!(self, Segment::Index(_))
    }

    pub fn coalesce(v: Vec<Field<'a>>) -> Segment<'a> {
        Segment::Coalesce(v)
    }

    pub fn is_coalesce(&self) -> bool {
        matches!(self, Segment::Coalesce(_))
    }
}

impl<'a> Display for Segment<'a> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Segment::Index(i) => write!(formatter, "{}", i),
            Segment::Field(Field {
                name,
                requires_quoting: false,
            }) => write!(formatter, "{}", name),
            Segment::Field(field) => write!(formatter, "{}", field),
            Segment::Coalesce(v) => write!(
                formatter,
                "({})",
                v.iter()
                    .map(|field| field.to_string())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        }
    }
}

impl<'a> From<&'a str> for Segment<'a> {
    fn from(name: &'a str) -> Self {
        Self::Field(name.into())
    }
}

impl<'a> From<isize> for Segment<'a> {
    fn from(value: isize) -> Self {
        Self::index(value)
    }
}

impl<'a> From<Vec<Field<'a>>> for Segment<'a> {
    fn from(value: Vec<Field<'a>>) -> Self {
        Self::coalesce(value)
    }
}

impl<'a> From<&'a SegmentBuf> for Segment<'a> {
    fn from(v: &'a SegmentBuf) -> Self {
        match v {
            SegmentBuf::Field(field) => Self::Field(field.into()),
            SegmentBuf::Index(i) => Self::index(*i),
            SegmentBuf::Coalesce(v) => Self::coalesce(v.iter().map(|field| field.into()).collect()),
        }
    }
}
