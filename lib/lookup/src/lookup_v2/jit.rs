use crate::lookup_v2::{BorrowedSegment, Path};
use std::borrow::Cow;
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
    escape_buffer: String,
}

impl<'a> JitLookup<'a> {
    pub fn new(path: &'a str) -> Self {
        Self {
            chars: path.char_indices(),
            path,
            state: JitState::Start,
            escape_buffer: String::new(),
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
    EscapedQuote,
    End,
}

impl<'a> Iterator for JitLookup<'a> {
    type Item = BorrowedSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.chars.next() {
                None => {
                    let result = match self.state {
                        JitState::Start
                        | JitState::IndexStart
                        | JitState::Index { .. }
                        | JitState::Quote { .. }
                        | JitState::EscapedQuote { .. } => Some(BorrowedSegment::Invalid),

                        JitState::Continue | JitState::Dot | JitState::End => None,

                        JitState::Field { start } => {
                            Some(BorrowedSegment::Field(Cow::Borrowed(&self.path[start..])))
                        }
                    };
                    self.state = JitState::End;
                    return result;
                }
                Some((index, c)) => {
                    let (result, state) = match self.state {
                        JitState::Start | JitState::Continue => match c {
                            '.' => (None, JitState::Dot),
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@' => {
                                (None, JitState::Field { start: index })
                            }
                            '[' => (None, JitState::IndexStart),
                            '\"' => (None, JitState::Quote { start: index + 1 }),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Dot => match c {
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@' => {
                                (None, JitState::Field { start: index })
                            }
                            '\"' => (None, JitState::Quote { start: index + 1 }),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Field { start } => match c {
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@' => {
                                (None, JitState::Field { start })
                            }
                            '.' => (
                                Some(Some(BorrowedSegment::Field(Cow::Borrowed(
                                    &self.path[start..index],
                                )))),
                                JitState::Dot,
                            ),
                            '[' => (
                                Some(Some(BorrowedSegment::Field(Cow::Borrowed(
                                    &self.path[start..index],
                                )))),
                                JitState::IndexStart,
                            ),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Quote { start } => match c {
                            '\"' => (
                                Some(Some(BorrowedSegment::Field(Cow::Borrowed(
                                    &self.path[start..index],
                                )))),
                                JitState::Continue,
                            ),
                            '\\' => {
                                // Character escaping requires copying chars to a new String.
                                // State is reverted back to the start of the quote to start over
                                // with the copy method (which is slower)
                                self.path = &self.path[start..];
                                self.chars = self.path.char_indices();
                                (None, JitState::EscapedQuote)
                            }
                            _ => (None, JitState::Quote { start }),
                        },
                        JitState::EscapedQuote => match c {
                            '\"' => (
                                (Some(Some(BorrowedSegment::Field(
                                    std::mem::take(&mut self.escape_buffer).into(),
                                )))),
                                JitState::Continue,
                            ),
                            '\\' => match self.chars.next() {
                                Some((_, c)) => match c {
                                    '\\' | '\"' => {
                                        self.escape_buffer.push(c);
                                        (None, JitState::EscapedQuote)
                                    }
                                    _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                                },
                                None => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                            },
                            _ => {
                                self.escape_buffer.push(c);
                                (None, JitState::EscapedQuote)
                            }
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
    use std::borrow::Cow;

    #[test]
    fn parsing() {
        let test_cases: Vec<(_, Vec<BorrowedSegment>)> = vec![
            ("", vec![BorrowedSegment::Invalid]),
            (".", vec![]),
            ("]", vec![BorrowedSegment::Invalid]),
            ("]foo", vec![BorrowedSegment::Invalid]),
            ("..", vec![BorrowedSegment::Invalid]),
            ("...", vec![BorrowedSegment::Invalid]),
            ("f", vec![BorrowedSegment::Field(Cow::from("f"))]),
            (".f", vec![BorrowedSegment::Field(Cow::from("f"))]),
            (".[", vec![BorrowedSegment::Invalid]),
            ("f.", vec![BorrowedSegment::Field(Cow::from("f"))]),
            ("foo", vec![BorrowedSegment::Field(Cow::from("foo"))]),
            (
                "ec2.metadata.\"availability-zone\"",
                vec![
                    BorrowedSegment::Field(Cow::from("ec2")),
                    BorrowedSegment::Field(Cow::from("metadata")),
                    BorrowedSegment::Field(Cow::from("availability-zone")),
                ],
            ),
            (".foo", vec![BorrowedSegment::Field(Cow::from("foo"))]),
            (
                ".@timestamp",
                vec![BorrowedSegment::Field(Cow::from("@timestamp"))],
            ),
            (
                "foo[",
                vec![
                    BorrowedSegment::Field(Cow::from("foo")),
                    BorrowedSegment::Invalid,
                ],
            ),
            ("foo$", vec![BorrowedSegment::Invalid]),
            (
                "\"$peci@l chars\"",
                vec![BorrowedSegment::Field(Cow::from("$peci@l chars"))],
            ),
            (
                ".foo.foo bar",
                vec![
                    BorrowedSegment::Field(Cow::from("foo")),
                    BorrowedSegment::Invalid,
                ],
            ),
            (
                ".foo.\"foo bar\".bar",
                vec![
                    BorrowedSegment::Field(Cow::from("foo")),
                    BorrowedSegment::Field(Cow::from("foo bar")),
                    BorrowedSegment::Field(Cow::from("bar")),
                ],
            ),
            ("[1]", vec![BorrowedSegment::Index(1)]),
            ("[42]", vec![BorrowedSegment::Index(42)]),
            (".[42]", vec![BorrowedSegment::Invalid]),
            (
                "[42].foo",
                vec![
                    BorrowedSegment::Index(42),
                    BorrowedSegment::Field(Cow::from("foo")),
                ],
            ),
            (
                "[42]foo",
                vec![
                    BorrowedSegment::Index(42),
                    BorrowedSegment::Field(Cow::from("foo")),
                ],
            ),
            ("[-1]", vec![BorrowedSegment::Invalid]),
            ("[-42]", vec![BorrowedSegment::Invalid]),
            (".[-42]", vec![BorrowedSegment::Invalid]),
            ("[-42].foo", vec![BorrowedSegment::Invalid]),
            ("[-42]foo", vec![BorrowedSegment::Invalid]),
            (
                ".\"[42]. {}-_\"",
                vec![BorrowedSegment::Field(Cow::from("[42]. {}-_"))],
            ),
            (
                "\"a\\\"a\"",
                vec![BorrowedSegment::Field(Cow::from("a\"a"))],
            ),
            (
                ".\"a\\\"a\"",
                vec![BorrowedSegment::Field(Cow::from("a\"a"))],
            ),
            (
                ".foo.\"a\\\"a\".\"b\\\\b\".bar",
                vec![
                    BorrowedSegment::Field(Cow::from("foo")),
                    BorrowedSegment::Field(Cow::from("a\"a")),
                    BorrowedSegment::Field(Cow::from("b\\b")),
                    BorrowedSegment::Field(Cow::from("bar")),
                ],
            ),
        ];

        for (path, expected) in test_cases {
            let segments: Vec<_> = JitPath::new(path).segment_iter().collect();
            assert_eq!(segments, expected)
        }
    }
}
