mod jit;

use std::borrow::Cow;
use std::iter::Cloned;
use std::slice::Iter;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use self::jit::{JitLookup, JitPath};

/// Syntactic sugar for creating a pre-parsed path.
///
/// Example: `path!("foo", 4, "bar")` is the pre-parsed path of `foo[4].bar`
#[macro_export]
macro_rules! path {
    ($($segment:expr),*) => {{
           &[$(lookup::lookup_v2::BorrowedSegment::from($segment),)*]
    }};
}

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
        if self.segments.is_empty() {
            serializer.serialize_str("<invalid>")
        } else {
            let path = self
                .segments
                .iter()
                .enumerate()
                .map(|(i, segment)| match segment {
                    OwnedSegment::Field(field) => {
                        let needs_quotes = field
                            .chars()
                            .any(|c| !matches!(c, 'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@'));
                        // Allocate enough to fit the field, a `.` and two `"` characters. This
                        // should suffice for the majority of cases when no escape sequence is used.
                        let mut string = String::with_capacity(field.as_bytes().len() + 3);
                        if i != 0 {
                            string.push('.');
                        }
                        if needs_quotes {
                            string.push('"');
                            for c in field.chars() {
                                if matches!(c, '"' | '\\') {
                                    string.push('\\');
                                }
                                string.push(c);
                            }
                            string.push('"');
                            string
                        } else {
                            string.push_str(field);
                            string
                        }
                    }
                    OwnedSegment::Index(index) => format!("[{}]", index),
                    OwnedSegment::Invalid => {
                        (if i == 0 { "<invalid>" } else { ".<invalid>" }).to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("");
            serializer.serialize_str(&path)
        }
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

/// Use if you want to pre-parse paths so it can be used multiple times.
/// The return value implements `Path` so it can be used directly.
pub fn parse_path(path: &str) -> OwnedPath {
    let segments = JitPath::new(path)
        .segment_iter()
        .map(|segment| segment.into())
        .collect();
    OwnedPath { segments }
}

#[derive(Clone)]
pub struct PathConcat<A, B> {
    a: A,
    b: B,
}

impl<'a, A: Path<'a>, B: Path<'a>> Path<'a> for PathConcat<A, B> {
    type Iter = std::iter::Chain<A::Iter, B::Iter>;

    fn segment_iter(&self) -> Self::Iter {
        self.a.segment_iter().chain(self.b.segment_iter())
    }
}

/// A path is simply the data describing how to look up a value.
/// This should only be implemented for types that are very cheap to clone, such as references.
pub trait Path<'a>: Clone {
    type Iter: Iterator<Item = BorrowedSegment<'a>>;

    fn segment_iter(&self) -> Self::Iter;

    fn concat<T: Path<'a>>(&self, path: T) -> PathConcat<Self, T> {
        PathConcat {
            a: self.clone(),
            b: path,
        }
    }
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

impl<'a> Path<'a> for &'a str {
    type Iter = JitLookup<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitPath::new(self).segment_iter()
    }
}

impl<'a> From<&'a str> for BorrowedSegment<'a> {
    fn from(field: &'a str) -> Self {
        BorrowedSegment::field(field)
    }
}

impl From<usize> for BorrowedSegment<'_> {
    fn from(index: usize) -> Self {
        BorrowedSegment::index(index)
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum OwnedSegment {
    Field(String),
    Index(usize),
    Invalid,
}

impl OwnedSegment {
    pub fn field(value: &str) -> OwnedSegment {
        OwnedSegment::Field(value.into())
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
            OwnedSegment::Field(value) => BorrowedSegment::Field(value.as_str().into()),
            OwnedSegment::Index(value) => BorrowedSegment::Index(*value),
            OwnedSegment::Invalid => BorrowedSegment::Invalid,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BorrowedSegment<'a> {
    Field(Cow<'a, str>),
    Index(usize),
    Invalid,
}

impl BorrowedSegment<'_> {
    pub fn field(value: &str) -> BorrowedSegment {
        BorrowedSegment::Field(Cow::Borrowed(value))
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
            BorrowedSegment::Field(value) => OwnedSegment::Field((*value).to_owned()),
            BorrowedSegment::Index(value) => OwnedSegment::Index(value),
            BorrowedSegment::Invalid => OwnedSegment::Invalid,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn owned_path_serialize() {
        let test_cases = [
            ("", "<invalid>"),
            ("]", "<invalid>"),
            ("]foo", "<invalid>"),
            ("..", "<invalid>"),
            ("...", "<invalid>"),
            ("f", "f"),
            ("foo", "foo"),
            (
                r#"ec2.metadata."availability-zone""#,
                r#"ec2.metadata."availability-zone""#,
            ),
            ("@timestamp", "@timestamp"),
            ("foo[", "foo.<invalid>"),
            ("foo$", "<invalid>"),
            (r#""$peci@l chars""#, r#""$peci@l chars""#),
            ("foo.foo bar", "foo.<invalid>"),
            (r#"foo."foo bar".bar"#, r#"foo."foo bar".bar"#),
            ("[1]", "[1]"),
            ("[42]", "[42]"),
            ("foo.[42]", "foo.<invalid>"),
            ("[42].foo", "[42].foo"),
            ("[-1]", "<invalid>"),
            ("[-42]", "<invalid>"),
            ("[-42].foo", "<invalid>"),
            ("[-42]foo", "<invalid>"),
            (r#""[42]. {}-_""#, r#""[42]. {}-_""#),
            (r#""a\"a""#, r#""a\"a""#),
            (r#"foo."a\"a"."b\\b".bar"#, r#"foo."a\"a"."b\\b".bar"#),
            ("<invalid>", "<invalid>"),
            (r#""🤖""#, r#""🤖""#),
        ];

        for (path, expected) in test_cases {
            let path = parse_path(path);
            let path = serde_json::to_string(&path).unwrap();
            let path = serde_json::from_str::<serde_json::Value>(&path).unwrap();
            assert_eq!(path, expected);
        }
    }
}
