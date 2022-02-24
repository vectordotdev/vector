// pub struct ParsedPath {
//     segments: Vec<OwnedSegment>,
// }

use std::str::CharIndices;

pub struct JitPath<'a> {
    path: &'a str,
}

impl JitPath<'_> {
    pub fn new(path: &str) -> JitPath {
        JitPath { path }
    }
}

// TODO: can probably just implement this for &str
impl<'a> Path<'a> for JitPath<'a> {
    type Iter = JitLookup<'a>;

    fn iter(&self) -> Self::Iter {
        JitLookup {
            path: &self.path,
            chars: self.path.char_indices(),
            state: JitState::Start,
        }
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

/// A path is simply the data describing how to look up a value
pub trait Path<'a> {
    type Iter: Iterator<Item = BorrowedSegment<'a>>;

    fn iter(&self) -> Self::Iter;
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

// enum OwnedSegment {
//     Field(String),
//     Index(isize),
// }

#[derive(Debug, PartialEq, Eq)]
pub enum BorrowedSegment<'a> {
    Field(&'a str),
    Index(isize),
    Invalid,
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
            let segments: Vec<_> = JitPath::new(path).iter().collect();
            assert_eq!(segments, expected)
        }
    }
}
