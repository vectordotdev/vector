// pub struct ParsedPath {
//     segments: Vec<OwnedSegment>,
// }

use std::iter::Cloned;
use std::slice::Iter;
use std::str::CharIndices;

/// Use if you want to pre-parse paths so it can be used multiple times
/// The return value implements `Path` so it can be used directly
pub fn parse_path(path: &str) -> Vec<OwnedSegment> {
    JitPath::new(path)
        .segment_iter()
        .map(|segment| segment.into())
        .collect()
}

pub struct JitPath<'a> {
    path: &'a str,
}

impl JitPath<'_> {
    pub fn new(path: &str) -> JitPath {
        JitPath { path }
    }
}

/// A path is simply the data describing how to look up a value
pub trait Path<'a> {
    type Iter: Iterator<Item = BorrowedSegment<'a>>;

    fn segment_iter(&self) -> Self::Iter;
}

impl<'a> Path<'a> for JitPath<'a> {
    type Iter = JitLookup<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitLookup {
            path: &self.path,
            chars: self.path.char_indices(),
            state: JitState::Start,
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
        // self.as_slice().iter().map(OwnedSegment::borrow)
        // unimplemented!()
        // self.iter().cloned()
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
        // unimplemented!()
        // self.iter().cloned()
    }
}

impl<'a> Path<'a> for &'a str {
    type Iter = JitLookup<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitPath::new(self).segment_iter()
    }
}

/// This is essentially an iterator over a `JitPath`.
pub struct JitLookup<'a> {
    path: &'a str,
    chars: CharIndices<'a>,
    state: JitState,
}

// TODO: maybe support whitespace around an index?
enum JitState {
    Start,
    Dot,
    IndexStart,
    IndexNegative { value: isize },
    Index { value: isize },
    Field { start: usize },
    Quote { start: usize },
    End,
}

impl<'a> Iterator for JitLookup<'a> {
    type Item = BorrowedSegment<'a>;

    fn next(&mut self) -> Option<BorrowedSegment<'a>> {
        loop {
            match self.chars.next() {
                None => {
                    let result = match self.state {
                        JitState::Start => None,
                        JitState::Dot => None,
                        JitState::IndexStart => Some(BorrowedSegment::Invalid),
                        JitState::IndexNegative { .. } => Some(BorrowedSegment::Invalid),
                        JitState::Index { .. } => Some(BorrowedSegment::Invalid),
                        JitState::Field { start } => {
                            Some(BorrowedSegment::Field(&self.path[start..]))
                        }
                        JitState::Quote { .. } => Some(BorrowedSegment::Invalid),
                        JitState::End => None,
                    };
                    self.state = JitState::End;
                    return result;
                }
                Some((index, c)) => {
                    let (result, state) = match self.state {
                        JitState::Start => match c {
                            '.' => (None, JitState::Dot),
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' => {
                                (None, JitState::Field { start: index })
                            }
                            '[' => (None, JitState::IndexStart),
                            '\"' => (None, JitState::Quote { start: index + 1 }),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Dot => match c {
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' => {
                                (None, JitState::Field { start: index })
                            }
                            '\"' => (None, JitState::Quote { start: index + 1 }),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Field { start } => match c {
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' => {
                                (None, JitState::Field { start: start })
                            }
                            '.' => (
                                Some(Some(BorrowedSegment::Field(&self.path[start..index]))),
                                JitState::Dot,
                            ),
                            '[' => (
                                Some(Some(BorrowedSegment::Field(&self.path[start..index]))),
                                JitState::IndexStart,
                            ),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Quote { start } => match c {
                            '\"' => (
                                Some(Some(BorrowedSegment::Field(&self.path[start..index]))),
                                JitState::Start,
                            ),
                            _ => (None, JitState::Quote { start }),
                        },
                        JitState::IndexStart => match c {
                            '0'..='9' => (
                                None,
                                JitState::Index {
                                    value: c as isize - '0' as isize,
                                },
                            ),
                            '-' => (None, JitState::IndexNegative { value: 0 }),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Index { value } => match c {
                            '0'..='9' => {
                                let new_digit = c as isize - '0' as isize;
                                (
                                    None,
                                    JitState::Index {
                                        value: value * 10 + new_digit,
                                    },
                                )
                            }
                            ']' => (Some(Some(BorrowedSegment::Index(value))), JitState::Start),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::IndexNegative { value } => match c {
                            '0'..='9' => {
                                let new_digit = c as isize - '0' as isize;
                                (
                                    None,
                                    JitState::IndexNegative {
                                        value: value * 10 - new_digit,
                                    },
                                )
                            }
                            ']' => (Some(Some(BorrowedSegment::Index(value))), JitState::Start),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::End => (Some(None), JitState::End),
                    };
                    self.state = state;
                    if let Some(result) = result {
                        return result;
                    }
                }
            }
        }
    }
}

// /// This tracks the current position in a path during a lookup. This should
// /// be very cheap to create and update. This is essentially an iterator over `BorrowedSegment`.
// pub trait Lookup2 {
//     // /// Returns the next segment, if there is one.
//     // /// This MUST NOT be called again after either `None` is returned, or `Some(BorrowedSegment::Invalid)`
//     // fn next<'a>(&'a mut self) -> Option<BorrowedSegment<'a>>;
// }

// pub struct LookupIter<'a, T> {
//     lookup: &'a mut T,
// }
//
// impl<'a, T: Lookup2> Iterator for LookupIter<'a, T> {
//     type Item = BorrowedSegment<'a>;
//
//     fn next(&mut self) -> Option<BorrowedSegment<'a>> {
//         self.lookup.next()
//         // None
//     }
// }

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum OwnedSegment {
    Field(String),
    Index(isize),
    Invalid,
}

impl<'a, 'b: 'a> From<&'b OwnedSegment> for BorrowedSegment<'a> {
    fn from(segment: &'b OwnedSegment) -> Self {
        match segment {
            OwnedSegment::Field(value) => BorrowedSegment::Field(value.as_str()),
            OwnedSegment::Index(value) => BorrowedSegment::Index(*value),
            OwnedSegment::Invalid => BorrowedSegment::Invalid,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BorrowedSegment<'a> {
    Field(&'a str),
    Index(isize),
    Invalid,
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
    use crate::lookup2::{BorrowedSegment, JitPath, Path};

    #[test]
    fn parsing() {
        let test_cases: Vec<(_, Vec<BorrowedSegment>)> = vec![
            ("", vec![]),
            (".", vec![]),
            ("]", vec![BorrowedSegment::Invalid]),
            ("]foo", vec![BorrowedSegment::Invalid]),
            ("..", vec![BorrowedSegment::Invalid]),
            ("...", vec![BorrowedSegment::Invalid]),
            ("f", vec![BorrowedSegment::Field("f")]),
            (".f", vec![BorrowedSegment::Field("f")]),
            (".[", vec![BorrowedSegment::Invalid]),
            ("f.", vec![BorrowedSegment::Field("f")]),
            ("foo", vec![BorrowedSegment::Field("foo")]),
            (".foo", vec![BorrowedSegment::Field("foo")]),
            (
                "foo[",
                vec![BorrowedSegment::Field("foo"), BorrowedSegment::Invalid],
            ),
            ("foo$", vec![BorrowedSegment::Invalid]),
            (
                "\"$peci@l chars\"",
                vec![BorrowedSegment::Field("$peci@l chars")],
            ),
            (
                ".foo.foo bar",
                vec![BorrowedSegment::Field("foo"), BorrowedSegment::Invalid],
            ),
            (
                ".foo.\"foo bar\".bar",
                vec![
                    BorrowedSegment::Field("foo"),
                    BorrowedSegment::Field("foo bar"),
                    BorrowedSegment::Field("bar"),
                ],
            ),
            ("[1]", vec![BorrowedSegment::Index(1)]),
            ("[42]", vec![BorrowedSegment::Index(42)]),
            (".[42]", vec![BorrowedSegment::Invalid]),
            (
                "[42].foo",
                vec![BorrowedSegment::Index(42), BorrowedSegment::Field("foo")],
            ),
            (
                "[42]foo",
                vec![BorrowedSegment::Index(42), BorrowedSegment::Field("foo")],
            ),
            ("[-1]", vec![BorrowedSegment::Index(-1)]),
            ("[-42]", vec![BorrowedSegment::Index(-42)]),
            (".[-42]", vec![BorrowedSegment::Invalid]),
            (
                "[-42].foo",
                vec![BorrowedSegment::Index(-42), BorrowedSegment::Field("foo")],
            ),
            (
                "[-42]foo",
                vec![BorrowedSegment::Index(-42), BorrowedSegment::Field("foo")],
            ),
            (
                ".\"[42]. {}-_\"",
                vec![BorrowedSegment::Field("[42]. {}-_")],
            ),
        ];

        for (path, expected) in test_cases {
            let segments: Vec<_> = JitPath::new(path).segment_iter().collect();
            assert_eq!(segments, expected)
        }
    }
}
