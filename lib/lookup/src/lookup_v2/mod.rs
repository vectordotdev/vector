mod borrowed;
mod concat;
mod jit;
mod owned;

use self::jit::{JitLookup, JitPath};

pub use borrowed::BorrowedSegment;
pub use concat::PathConcat;
pub use owned::{OwnedPath, OwnedSegment};

/// Syntactic sugar for creating a pre-parsed path.
///
/// Example: `path!("foo", 4, "bar")` is the pre-parsed path of `foo[4].bar`
#[macro_export]
macro_rules! path {
    ($($segment:expr),*) => {{
           &[$($crate::lookup_v2::BorrowedSegment::from($segment),)*]
    }};
}

/// Syntactic sugar for creating a pre-parsed owned path.
///
/// This allocates and will be slower than using `path!`. Prefer that when possible.
/// The return value must be borrowed to get a value that implements `Path`.
///
/// Example: `owned_path!("foo", 4, "bar")` is the pre-parsed path of `foo[4].bar`
#[macro_export]
macro_rules! owned_path {
    ($($segment:expr),*) => {{
           $crate::lookup_v2::OwnedPath::from(vec![$($crate::lookup_v2::OwnedSegment::from($segment),)*])
    }};
}

/// Use if you want to pre-parse paths so it can be used multiple times.
/// The return value (when borrowed) implements `Path` so it can be used directly.
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

    fn concat<T: Path<'a>>(&self, path: T) -> PathConcat<Self, T> {
        PathConcat {
            a: self.clone(),
            b: path,
        }
    }

    fn eq(&self, other: impl Path<'a>) -> bool {
        self.segment_iter().eq(other.segment_iter())
    }
}

impl<'a> Path<'a> for &'a str {
    type Iter = JitLookup<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitPath::new(self).segment_iter()
    }
}
// <<<<<<< HEAD
//
// impl<'a> From<&'a str> for BorrowedSegment<'a> {
//     fn from(field: &'a str) -> Self {
//         BorrowedSegment::field(field)
//     }
// }
//
// impl<'a> From<&'a String> for BorrowedSegment<'a> {
//     fn from(field: &'a String) -> Self {
//         BorrowedSegment::field(field.as_str())
//     }
// }
//
// impl From<usize> for BorrowedSegment<'_> {
//     fn from(index: usize) -> Self {
//         BorrowedSegment::index(index)
//     }
// }
//
// #[derive(Debug, PartialEq, Eq, Clone)]
// pub enum OwnedSegment {
//     Field(String),
//     Index(usize),
//     Invalid,
// }
//
// impl OwnedSegment {
//     pub fn field(value: &str) -> OwnedSegment {
//         OwnedSegment::Field(value.into())
//     }
//     pub fn index(value: usize) -> OwnedSegment {
//         OwnedSegment::Index(value)
//     }
//     pub fn is_field(&self) -> bool {
//         matches!(self, OwnedSegment::Field(_))
//     }
//     pub fn is_index(&self) -> bool {
//         matches!(self, OwnedSegment::Index(_))
//     }
//     pub fn is_invalid(&self) -> bool {
//         matches!(self, OwnedSegment::Invalid)
//     }
// }
//
// impl<'a, 'b: 'a> From<&'b OwnedSegment> for BorrowedSegment<'a> {
//     fn from(segment: &'b OwnedSegment) -> Self {
//         match segment {
//             OwnedSegment::Field(value) => BorrowedSegment::Field(value.as_str().into()),
//             OwnedSegment::Index(value) => BorrowedSegment::Index(*value),
//             OwnedSegment::Invalid => BorrowedSegment::Invalid,
//         }
//     }
// }
//
// #[derive(Debug, PartialEq, Eq, Clone)]
// pub enum BorrowedSegment<'a> {
//     Field(Cow<'a, str>),
//     Index(usize),
//     Invalid,
// }
//
// impl BorrowedSegment<'_> {
//     pub const fn field(value: &str) -> BorrowedSegment {
//         BorrowedSegment::Field(Cow::Borrowed(value))
//     }
//     pub const fn index(value: usize) -> BorrowedSegment<'static> {
//         BorrowedSegment::Index(value)
//     }
//     pub fn is_field(&self) -> bool {
//         matches!(self, BorrowedSegment::Field(_))
//     }
//     pub fn is_index(&self) -> bool {
//         matches!(self, BorrowedSegment::Index(_))
//     }
//     pub fn is_invalid(&self) -> bool {
//         matches!(self, BorrowedSegment::Invalid)
//     }
// }
//
// impl<'a> From<BorrowedSegment<'a>> for OwnedSegment {
//     fn from(x: BorrowedSegment<'a>) -> Self {
//         match x {
//             BorrowedSegment::Field(value) => OwnedSegment::Field((*value).to_owned()),
//             BorrowedSegment::Index(value) => OwnedSegment::Index(value),
//             BorrowedSegment::Invalid => OwnedSegment::Invalid,
//         }
//     }
// }
//
// #[cfg(any(test, feature = "arbitrary"))]
// impl quickcheck::Arbitrary for BorrowedSegment<'static> {
//     fn arbitrary(g: &mut quickcheck::Gen) -> Self {
//         match usize::arbitrary(g) % 2 {
//             0 => BorrowedSegment::Index(usize::arbitrary(g) % 20),
//             _ => BorrowedSegment::Field(String::arbitrary(g).into()),
//         }
//     }
//
//     fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
//         match self {
//             BorrowedSegment::Invalid => Box::new(std::iter::empty()),
//             BorrowedSegment::Index(index) => Box::new(index.shrink().map(BorrowedSegment::Index)),
//             BorrowedSegment::Field(field) => Box::new(
//                 field
//                     .to_string()
//                     .shrink()
//                     .map(|f| BorrowedSegment::Field(f.into())),
//             ),
//         }
//     }
// }
//
// #[cfg(test)]
// mod test {
//     use super::*;
//
//     #[test]
//     fn owned_path_serialize() {
//         let test_cases = [
//             ("", "<invalid>"),
//             ("]", "<invalid>"),
//             ("]foo", "<invalid>"),
//             ("..", "<invalid>"),
//             ("...", "<invalid>"),
//             ("f", "f"),
//             ("foo", "foo"),
//             (
//                 r#"ec2.metadata."availability-zone""#,
//                 r#"ec2.metadata."availability-zone""#,
//             ),
//             ("@timestamp", "@timestamp"),
//             ("foo[", "foo.<invalid>"),
//             ("foo$", "<invalid>"),
//             (r#""$peci@l chars""#, r#""$peci@l chars""#),
//             ("foo.foo bar", "foo.<invalid>"),
//             (r#"foo."foo bar".bar"#, r#"foo."foo bar".bar"#),
//             ("[1]", "[1]"),
//             ("[42]", "[42]"),
//             ("foo.[42]", "foo.<invalid>"),
//             ("[42].foo", "[42].foo"),
//             ("[-1]", "<invalid>"),
//             ("[-42]", "<invalid>"),
//             ("[-42].foo", "<invalid>"),
//             ("[-42]foo", "<invalid>"),
//             (r#""[42]. {}-_""#, r#""[42]. {}-_""#),
//             (r#""a\"a""#, r#""a\"a""#),
//             (r#"foo."a\"a"."b\\b".bar"#, r#"foo."a\"a"."b\\b".bar"#),
//             ("<invalid>", "<invalid>"),
//             (r#""🤖""#, r#""🤖""#),
//         ];
//
//         for (path, expected) in test_cases {
//             let path = parse_path(path);
//             let path = serde_json::to_string(&path).unwrap();
//             let path = serde_json::from_str::<serde_json::Value>(&path).unwrap();
//             assert_eq!(path, expected);
//         }
//     }
// }
// =======
// >>>>>>> master
