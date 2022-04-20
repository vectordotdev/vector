mod jit;

use crate::lookup_v2::jit::{JitLookup, JitPath};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::fmt::Display;
use std::iter::Cloned;
use std::slice::Iter;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnedPath {
    pub segments: Vec<OwnedSegment>,
}

impl<'de> Deserialize<'de> for OwnedPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path: String = Deserialize::deserialize(deserializer)?;
        Ok(parse_path(&path))
    }
}

impl Serialize for OwnedPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        unimplemented!()
        // serializer.serialize_str(&self.to_string())
    }
}

impl OwnedPath {
    pub fn root() -> Self {
        vec![].into()
    }

    pub fn push_field(&mut self, field: &str) {
        self.segments.push(OwnedSegment::field(field));
    }

    pub fn with_field_appended(&self, field: &str) -> Self {
        let mut new_path = self.clone();
        new_path.push_field(field);
        new_path
    }

    pub fn push_index(&mut self, index: usize) {
        self.segments.push(OwnedSegment::index(index));
    }

    pub fn with_index_appended(&self, index: usize) -> Self {
        let mut new_path = self.clone();
        new_path.push_index(index);
        new_path
    }

    pub fn single_field(field: &str) -> Self {
        vec![OwnedSegment::field(field)].into()
    }
}

impl From<Vec<OwnedSegment>> for OwnedPath {
    fn from(segments: Vec<OwnedSegment>) -> Self {
        Self { segments }
    }
}

// impl Display for OwnedPath {
//     fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
//         if self.segments.is_empty() {
//             write!(formatter, ".")
//         } else {
//             self.segments
//                 .iter()
//                 .try_for_each(|segment| segment.fmt(formatter))
//         }
//     }
// }

/// Use if you want to pre-parse paths so it can be used multiple times.
/// The return value implements `Path` so it can be used directly.
pub fn parse_path(path: &str) -> OwnedPath {
    let segments = JitPath::new(path)
        .segment_iter()
        .map(|segment| segment.into())
        .collect();
    OwnedPath { segments }
}

/// A path is simply the data describing how to look up a value.
/// This should only be implemented for types that are very cheap to clone, such as references.
pub trait Path<'a>: Clone {
    type Iter: Iterator<Item = BorrowedSegment<'a>>;

    fn segment_iter(&self) -> Self::Iter;
}

impl<'a> Path<'a> for &'a Vec<OwnedSegment> {
    type Iter = OwnedSegmentSliceIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        OwnedSegmentSliceIter {
            segments: self.as_slice(),
            index: 0,
        }
    }
}

impl<'a> Path<'a> for &'a OwnedPath {
    type Iter = OwnedSegmentSliceIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        (&self.segments).segment_iter()
    }
}

pub struct OwnedSegmentSliceIter<'a> {
    segments: &'a [OwnedSegment],
    index: usize,
}

impl<'a> Iterator for OwnedSegmentSliceIter<'a> {
    type Item = BorrowedSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let output = self.segments.get(self.index).map(|x| x.into());
        self.index += 1;
        output
    }
}

impl<'a, 'b: 'a> Path<'a> for &'b Vec<BorrowedSegment<'a>> {
    type Iter = Cloned<Iter<'a, BorrowedSegment<'a>>>;

    fn segment_iter(&self) -> Self::Iter {
        self.as_slice().iter().cloned()
    }
}

impl<'a, 'b: 'a> Path<'a> for &'b [BorrowedSegment<'a>] {
    type Iter = Cloned<Iter<'a, BorrowedSegment<'a>>>;

    fn segment_iter(&self) -> Self::Iter {
        self.iter().cloned()
    }
}

impl<'a, 'b: 'a, const A: usize> Path<'a> for &'b [BorrowedSegment<'a>; A] {
    type Iter = Cloned<Iter<'a, BorrowedSegment<'a>>>;

    fn segment_iter(&self) -> Self::Iter {
        self.iter().cloned()
    }
}

impl<'a> Path<'a> for &'a str {
    type Iter = JitLookup<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitPath::new(self).segment_iter()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum OwnedSegment {
    Field(OwnedField),
    Index(usize),
    Invalid,
}

impl OwnedSegment {
    pub fn field(value: &str) -> OwnedSegment {
        println!(
            "OwnedSegment::field value: {}, start with quote: {}",
            value,
            value.starts_with('\"')
        );
        JitLookup::new(value)
            .into_iter()
            .next()
            .unwrap_or(BorrowedSegment::Invalid)
            .into()
    }
    pub fn index(value: usize) -> OwnedSegment {
        OwnedSegment::Index(value)
    }
    pub fn is_field(&self) -> bool {
        matches!(self, OwnedSegment::Field(_))
    }
    pub fn is_index(&self) -> bool {
        matches!(self, OwnedSegment::Index(_))
    }
    pub fn is_invalid(&self) -> bool {
        matches!(self, OwnedSegment::Invalid)
    }
}

impl<'a, 'b: 'a> From<&'b OwnedSegment> for BorrowedSegment<'a> {
    fn from(segment: &'b OwnedSegment) -> Self {
        match segment {
            OwnedSegment::Field(value) => BorrowedSegment::Field(value.into()),
            OwnedSegment::Index(value) => BorrowedSegment::Index(*value),
            OwnedSegment::Invalid => BorrowedSegment::Invalid,
        }
    }
}

// impl Display for OwnedSegment {
//     fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
//         match self {
//             Self::Field(field) => write!(formatter, ".{}", field),
//             Self::Index(index) => write!(formatter, "[{}]", index),
//             Self::Invalid => write!(formatter, ".<invalid>"),
//         }
//     }
// }

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum OwnedField {
    Quoted(String),
    Regular(String),
}

// impl Display for OwnedField {
//     fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
//         match self {
//             Self::Quoted(field) => {
//                 let mut string = String::from('"');
//                 string.reserve(field.as_bytes().len());
//                 for c in field.chars() {
//                     if matches!(c, '"' | '\\') {
//                         string.push('\\');
//                     }
//                     string.push(c);
//                 }
//                 string.push('"');
//                 formatter.write_str(&string)
//             }
//             Self::Regular(field) => formatter.write_str(field),
//         }
//     }
// }

impl<'a, 'b: 'a> From<&'b OwnedField> for BorrowedField<'a> {
    fn from(segment: &'b OwnedField) -> Self {
        match segment {
            OwnedField::Quoted(value) => Self::Quoted(value.into()),
            OwnedField::Regular(value) => Self::Regular(value.into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BorrowedSegment<'a> {
    Field(BorrowedField<'a>),
    Index(usize),
    Invalid,
}

impl BorrowedSegment<'_> {
    pub fn field(value: &str) -> BorrowedSegment {
        JitLookup::new(value)
            .into_iter()
            .next()
            .unwrap_or(BorrowedSegment::Invalid)
    }
    pub fn index(value: usize) -> BorrowedSegment<'static> {
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

impl<'a> From<BorrowedSegment<'a>> for OwnedSegment {
    fn from(x: BorrowedSegment<'a>) -> Self {
        match x {
            BorrowedSegment::Field(value) => Self::Field(value.into()),
            BorrowedSegment::Index(value) => Self::Index(value),
            BorrowedSegment::Invalid => Self::Invalid,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BorrowedField<'a> {
    Quoted(Cow<'a, str>),
    Regular(Cow<'a, str>),
}

// impl<'a> Display for BorrowedField<'a> {
//     fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
//         match self {
//             Self::Quoted(field) => {
//                 let mut string = String::from('"');
//                 string.reserve(field.as_bytes().len());
//                 for c in field.chars() {
//                     if matches!(c, '"' | '\\') {
//                         string.push('\\');
//                     }
//                     string.push(c);
//                 }
//                 string.push('"');
//                 formatter.write_str(&string)
//             }
//             Self::Regular(field) => formatter.write_str(field),
//         }
//     }
// }

impl<'a> From<BorrowedField<'a>> for OwnedField {
    fn from(x: BorrowedField<'a>) -> Self {
        match x {
            BorrowedField::Quoted(value) => Self::Quoted(value.to_string()),
            BorrowedField::Regular(value) => Self::Regular(value.to_string()),
        }
    }
}

impl<'a> AsRef<str> for BorrowedField<'a> {
    fn as_ref(&self) -> &str {
        match self {
            Self::Quoted(field) => field.as_ref(),
            Self::Regular(field) => field.as_ref(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn owned_path_display() {
        let test_cases = [
            ("", ".<invalid>"),
            (".", "."),
            ("]", ".<invalid>"),
            ("]foo", ".<invalid>"),
            ("..", ".<invalid>"),
            ("...", ".<invalid>"),
            ("f", ".f"),
            (".f", ".f"),
            (".[", ".<invalid>"),
            ("foo", ".foo"),
            (r#""no_quotes_needed""#, ".no_quotes_needed"),
            (
                r#"ec2.metadata."availability-zone""#,
                r#".ec2.metadata."availability-zone""#,
            ),
            (".foo", ".foo"),
            (".@timestamp", ".@timestamp"),
            ("foo[", ".foo.<invalid>"),
            ("foo$", ".<invalid>"),
            (r#""$peci@l chars""#, r#"."$peci@l chars""#),
            (".foo.foo bar", ".foo.<invalid>"),
            (r#".foo."foo bar".bar"#, r#".foo."foo bar".bar"#),
            ("[1]", "[1]"),
            ("[42]", "[42]"),
            (".[42]", ".<invalid>"),
            ("[42].foo", "[42].foo"),
            ("[-1]", ".<invalid>"),
            ("[-42]", ".<invalid>"),
            (".[-42]", ".<invalid>"),
            ("[-42].foo", ".<invalid>"),
            ("[-42]foo", ".<invalid>"),
            (r#"."[42]. {}-_""#, r#"."[42]. {}-_""#),
            (r#""a\"a""#, r#"."a\"a""#),
            (r#"."a\"a""#, r#"."a\"a""#),
            (r#".foo."a\"a"."b\\b".bar"#, r#".foo."a\"a"."b\\b".bar"#),
            (".<invalid>", ".<invalid>"),
            (r#"."🤖""#, r#"."🤖""#),
        ];

        for (path, expected) in test_cases {
            let path = parse_path(path);
            // assert_eq!(format!("{}", path), expected);
        }
    }
}
