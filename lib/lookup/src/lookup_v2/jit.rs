use crate::lookup_v2::{BorrowedSegment, Path};
use std::str::CharIndices;

#[derive(Clone)]
pub struct JitPath<'a> {
    path: &'a str,
}

impl JitPath<'_> {
    pub fn new(path: &str) -> JitPath {
        JitPath { path }
    }
}

/// This is essentially an iterator over a `JitPath`.
pub struct JitLookup<'a> {
    path: &'a str,
    chars: CharIndices<'a>,
    state: JitState,
}

impl<'a> JitLookup<'a> {
    pub fn new(path: &'a str) -> Self {
        Self {
            chars: path.char_indices(),
            path,
            state: JitState::Start,
        }
    }
}

impl<'a> Path<'a> for JitPath<'a> {
    type Iter = JitLookup<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitLookup::new(self.path)
    }
}

enum JitState {
    Start,
    Continue,
    Dot,
    IndexStart,
    Index { value: usize },
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
                        JitState::Start => Some(BorrowedSegment::Invalid),
                        JitState::Continue => None,
                        JitState::Dot => None,
                        JitState::IndexStart => Some(BorrowedSegment::Invalid),
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
                        JitState::Start | JitState::Continue => match c {
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
                                JitState::Continue,
                            ),
                            _ => (None, JitState::Quote { start }),
                        },
                        JitState::IndexStart => match c {
                            '0'..='9' => (
                                None,
                                JitState::Index {
                                    value: c as usize - '0' as usize,
                                },
                            ),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Index { value } => match c {
                            '0'..='9' => {
                                let new_digit = c as usize - '0' as usize;
                                (
                                    None,
                                    JitState::Index {
                                        value: value * 10 + new_digit,
                                    },
                                )
                            }
                            ']' => (
                                Some(Some(BorrowedSegment::Index(value))),
                                JitState::Continue,
                            ),
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

#[cfg(test)]
mod test {
    use crate::lookup_v2::{BorrowedSegment, JitPath, Path};

    #[test]
    fn parsing() {
        let test_cases: Vec<(_, Vec<BorrowedSegment>)> = vec![
            ("", vec![BorrowedSegment::Invalid]),
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
            ("[-1]", vec![BorrowedSegment::Invalid]),
            ("[-42]", vec![BorrowedSegment::Invalid]),
            (".[-42]", vec![BorrowedSegment::Invalid]),
            ("[-42].foo", vec![BorrowedSegment::Invalid]),
            ("[-42]foo", vec![BorrowedSegment::Invalid]),
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
