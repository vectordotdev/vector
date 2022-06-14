use crate::lookup_v2::{OwnedSegment, Path};
use std::borrow::Cow;
use std::iter::Cloned;
use std::slice::Iter;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BorrowedSegment<'a> {
    Field(Cow<'a, str>),
    Index(isize),
    CoalesceStart,
    CoalesceField(Cow<'a, str>),
    // This has an optional field since the parser would have to emit 2 segments in 1 state otherwise
    CoalesceEnd(Option<Cow<'a, str>>),
    Invalid,
}

impl BorrowedSegment<'_> {
    pub fn field(value: &str) -> BorrowedSegment {
        BorrowedSegment::Field(Cow::Borrowed(value))
    }
    pub fn index(value: isize) -> BorrowedSegment<'static> {
        BorrowedSegment::Index(value)
    }
    pub fn is_field(&self) -> bool {
        matches!(self, BorrowedSegment::Field(_))
    }
    pub fn is_index(&self) -> bool {
        matches!(self, BorrowedSegment::Index(_))
    }
    pub fn is_invalid(&self) -> bool {
        matches!(self, BorrowedSegment::Invalid)
    }
}

impl<'a> From<&'a str> for BorrowedSegment<'a> {
    fn from(field: &'a str) -> Self {
        BorrowedSegment::field(field)
    }
}

impl<'a> From<&'a String> for BorrowedSegment<'a> {
    fn from(field: &'a String) -> Self {
        BorrowedSegment::field(field.as_str())
    }
}

impl From<isize> for BorrowedSegment<'_> {
    fn from(index: isize) -> Self {
        BorrowedSegment::index(index)
    }
}

impl<'a, 'b: 'a> From<&'b OwnedSegment> for BorrowedSegment<'a> {
    fn from(segment: &'b OwnedSegment) -> Self {
        match segment {
            OwnedSegment::Field(value) => BorrowedSegment::Field(value.clone()),
            OwnedSegment::Index(value) => BorrowedSegment::Index(*value),
            OwnedSegment::Invalid => BorrowedSegment::Invalid,
            OwnedSegment::CoalesceStart => BorrowedSegment::CoalesceStart,
            OwnedSegment::CoalesceField(field) => BorrowedSegment::CoalesceField(field.clone()),
            OwnedSegment::CoalesceEnd(field) => {
                BorrowedSegment::CoalesceEnd(field.clone().map(|x| x.clone()))
            }
        }
    }
}

impl<'a, 'b> Path<'a> for &'b Vec<BorrowedSegment<'a>> {
    type Iter = Cloned<Iter<'b, BorrowedSegment<'a>>>;

    fn segment_iter(&self) -> Self::Iter {
        self.as_slice().iter().cloned()
    }
}

impl<'a, 'b> Path<'a> for &'b [BorrowedSegment<'a>] {
    type Iter = Cloned<Iter<'b, BorrowedSegment<'a>>>;

    fn segment_iter(&self) -> Self::Iter {
        self.iter().cloned()
    }
}

impl<'a, 'b, const A: usize> Path<'a> for &'b [BorrowedSegment<'a>; A] {
    type Iter = Cloned<Iter<'b, BorrowedSegment<'a>>>;

    fn segment_iter(&self) -> Self::Iter {
        self.iter().cloned()
    }
}

#[cfg(any(test, feature = "arbitrary"))]
impl quickcheck::Arbitrary for BorrowedSegment<'static> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        if bool::arbitrary(g) {
            if bool::arbitrary(g) {
                BorrowedSegment::Index((usize::arbitrary(g) % 20) as isize)
            } else {
                BorrowedSegment::Index(-((usize::arbitrary(g) % 20) as isize))
            }
        } else {
            BorrowedSegment::Field(String::arbitrary(g).into())
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            BorrowedSegment::Invalid => Box::new(std::iter::empty()),
            BorrowedSegment::Index(index) => Box::new(index.shrink().map(BorrowedSegment::Index)),
            BorrowedSegment::Field(field) => Box::new(
                field
                    .to_string()
                    .shrink()
                    .map(|f| BorrowedSegment::Field(f.into())),
            ),
            _ => todo!(),
        }
    }
}
