//!
//! This contains all the functionality for the JIT (Just In Time) features of the lookup path.
//! This allows parsing on the fly as it's needed.
//!
//! This contains a hand-written state machine to maximize performance of the parser.
//! This is important since a lot of parsing happens at runtime. Eventually Vector
//! should pre-compile all paths. Once that happens it might make sense to re-write in something
//! more readable.

use std::borrow::Cow;
use std::str::CharIndices;

use crate::lookup_v2::{BorrowedSegment, Path};

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
#[derive(Clone)]
pub struct JitLookup<'a> {
    path: &'a str,
    chars: CharIndices<'a>,
    state: JitState,
    escape_buffer: String,
    // keep track of the number of options in a coalesce to prevent size 1 coalesces
    coalesce_count: u32,
}

impl<'a> JitLookup<'a> {
    pub fn new(path: &'a str) -> Self {
        Self {
            chars: path.char_indices(),
            path,
            state: JitState::Start,
            escape_buffer: String::new(),
            coalesce_count: 0,
        }
    }
}

impl<'a> Path<'a> for JitPath<'a> {
    type Iter = JitLookup<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitLookup::new(self.path)
    }
}

#[derive(Clone)]
enum JitState {
    EventRoot,
    Start,
    Continue,
    Dot,
    IndexStart,
    NegativeIndex { value: isize },
    Index { value: isize },
    Field { start: usize },
    Quote { start: usize },
    EscapedQuote,
    CoalesceStart,
    CoalesceField { start: usize },
    CoalesceFieldEnd { start: usize, end: usize },
    CoalesceEscapedFieldEnd,
    CoalesceQuote { start: usize },
    CoalesceEscapedQuote,
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
                        | JitState::NegativeIndex { .. }
                        | JitState::Quote { .. }
                        | JitState::EscapedQuote { .. }
                        | JitState::CoalesceStart
                        | JitState::CoalesceField { .. }
                        | JitState::CoalesceFieldEnd { .. }
                        | JitState::CoalesceQuote { .. }
                        | JitState::CoalesceEscapedQuote { .. }
                        | JitState::CoalesceEscapedFieldEnd
                        | JitState::Dot => Some(BorrowedSegment::Invalid),

                        JitState::Continue | JitState::EventRoot | JitState::End => None,

                        JitState::Field { start } => {
                            Some(BorrowedSegment::Field(Cow::Borrowed(&self.path[start..])))
                        }
                    };
                    self.state = JitState::End;
                    return result;
                }
                Some((index, c)) => {
                    let (result, state) = match self.state {
                        JitState::Start => match c {
                            '.' => (None, JitState::EventRoot),
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@' => {
                                (None, JitState::Field { start: index })
                            }
                            '[' => (None, JitState::IndexStart),
                            '(' => (None, JitState::CoalesceStart),
                            '\"' => (None, JitState::Quote { start: index + 1 }),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Continue => match c {
                            '.' => (None, JitState::Dot),
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@' => {
                                (None, JitState::Field { start: index })
                            }
                            '[' => (None, JitState::IndexStart),
                            '(' => (None, JitState::CoalesceStart),
                            '\"' => (None, JitState::Quote { start: index + 1 }),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::EventRoot => match c {
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@' => {
                                (None, JitState::Field { start: index })
                            }
                            '[' => (None, JitState::IndexStart),
                            '(' => (None, JitState::CoalesceStart),
                            '\"' => (None, JitState::Quote { start: index + 1 }),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::Dot => match c {
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@' => {
                                (None, JitState::Field { start: index })
                            }
                            '(' => (None, JitState::CoalesceStart),
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
                                    value: c as isize - '0' as isize,
                                },
                            ),
                            '-' => (None, JitState::NegativeIndex { value: 0 }),
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
                            ']' => (
                                Some(Some(BorrowedSegment::Index(value))),
                                JitState::Continue,
                            ),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::NegativeIndex { value } => match c {
                            '0'..='9' => {
                                let new_digit = c as isize - '0' as isize;
                                (
                                    None,
                                    JitState::NegativeIndex {
                                        value: value * 10 - new_digit,
                                    },
                                )
                            }
                            ']' => (
                                Some(Some(BorrowedSegment::Index(value))),
                                JitState::Continue,
                            ),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::CoalesceStart => match c {
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@' => {
                                (None, JitState::CoalesceField { start: index })
                            }
                            ' ' => (None, JitState::CoalesceStart),
                            '\"' => (None, JitState::CoalesceQuote { start: index + 1 }),
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::CoalesceField { start } => match c {
                            'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@' => {
                                (None, JitState::CoalesceField { start })
                            }
                            ' ' => (None, JitState::CoalesceFieldEnd { start, end: index }),
                            '|' => {
                                self.coalesce_count += 1;
                                (
                                    Some(Some(BorrowedSegment::CoalesceField(Cow::Borrowed(
                                        &self.path[start..index],
                                    )))),
                                    JitState::CoalesceStart,
                                )
                            }
                            ')' => {
                                if self.coalesce_count == 0 {
                                    (Some(Some(BorrowedSegment::Invalid)), JitState::End)
                                } else {
                                    self.coalesce_count = 0;
                                    (
                                        Some(Some(BorrowedSegment::CoalesceEnd(Cow::Borrowed(
                                            &self.path[start..index],
                                        )))),
                                        JitState::Continue,
                                    )
                                }
                            }
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::CoalesceFieldEnd { start, end } => match c {
                            ' ' => (None, JitState::CoalesceFieldEnd { start, end }),
                            '|' => {
                                self.coalesce_count += 1;
                                (
                                    Some(Some(BorrowedSegment::CoalesceField(Cow::Borrowed(
                                        &self.path[(start as usize)..(end as usize)],
                                    )))),
                                    JitState::CoalesceStart,
                                )
                            }
                            ')' => {
                                if self.coalesce_count == 0 {
                                    (Some(Some(BorrowedSegment::Invalid)), JitState::End)
                                } else {
                                    self.coalesce_count = 0;
                                    (
                                        Some(Some(BorrowedSegment::CoalesceEnd(Cow::Borrowed(
                                            &self.path[(start as usize)..(end as usize)],
                                        )))),
                                        JitState::Continue,
                                    )
                                }
                            }
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::CoalesceEscapedFieldEnd => match c {
                            ' ' => (None, JitState::CoalesceEscapedFieldEnd),
                            '|' => {
                                self.coalesce_count += 1;
                                (
                                    (Some(Some(BorrowedSegment::CoalesceField(
                                        std::mem::take(&mut self.escape_buffer).into(),
                                    )))),
                                    JitState::CoalesceStart,
                                )
                            }
                            ')' => {
                                if self.coalesce_count == 0 {
                                    (Some(Some(BorrowedSegment::Invalid)), JitState::End)
                                } else {
                                    self.coalesce_count = 0;
                                    (
                                        (Some(Some(BorrowedSegment::CoalesceEnd(
                                            std::mem::take(&mut self.escape_buffer).into(),
                                        )))),
                                        JitState::Continue,
                                    )
                                }
                            }
                            _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                        },
                        JitState::CoalesceQuote { start } => match c {
                            '\"' => (None, JitState::CoalesceFieldEnd { start, end: index }),
                            '\\' => {
                                // Character escaping requires copying chars to a new String.
                                // State is reverted back to the start of the quote to start over
                                // with the copy method (which is slower)
                                self.path = &self.path[start..];
                                self.chars = self.path.char_indices();
                                (None, JitState::CoalesceEscapedQuote)
                            }
                            _ => (None, JitState::CoalesceQuote { start }),
                        },
                        JitState::CoalesceEscapedQuote => match c {
                            '\"' => (None, JitState::CoalesceEscapedFieldEnd),
                            '\\' => match self.chars.next() {
                                Some((_, c)) => match c {
                                    '\\' | '\"' => {
                                        self.escape_buffer.push(c);
                                        (None, JitState::CoalesceEscapedQuote)
                                    }
                                    _ => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                                },
                                None => (Some(Some(BorrowedSegment::Invalid)), JitState::End),
                            },
                            _ => {
                                self.escape_buffer.push(c);
                                (None, JitState::CoalesceEscapedQuote)
                            }
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
    use crate::lookup_v2::{OwnedPath, OwnedSegment, Path};
    use crate::owned_path;

    #[test]
    fn parsing() {
        let test_cases: Vec<(_, OwnedPath)> = vec![
            ("", owned_path!(OwnedSegment::Invalid)),
            (".", owned_path!()),
            ("]", owned_path!(OwnedSegment::Invalid)),
            ("]foo", owned_path!(OwnedSegment::Invalid)),
            ("..", owned_path!(OwnedSegment::Invalid)),
            ("...", owned_path!(OwnedSegment::Invalid)),
            ("f", owned_path!("f")),
            (".f", owned_path!("f")),
            (".[", owned_path!(OwnedSegment::Invalid)),
            ("f.", owned_path!("f", OwnedSegment::Invalid)),
            ("foo", owned_path!("foo")),
            (
                "ec2.metadata.\"availability-zone\"",
                owned_path!("ec2", "metadata", "availability-zone"),
            ),
            (".foo", owned_path!("foo")),
            (".@timestamp", owned_path!("@timestamp")),
            ("foo[", owned_path!("foo", OwnedSegment::Invalid)),
            ("foo$", owned_path!(OwnedSegment::Invalid)),
            ("\"$peci@l chars\"", owned_path!("$peci@l chars")),
            (".foo.foo bar", owned_path!("foo", OwnedSegment::Invalid)),
            (".foo.\"foo bar\".bar", owned_path!("foo", "foo bar", "bar")),
            ("[1]", owned_path!(1)),
            ("[42]", owned_path!(42)),
            (".[42]", owned_path!(42)),
            ("[42].foo", owned_path!(42, "foo")),
            ("foo.[42]", owned_path!("foo", OwnedSegment::Invalid)),
            ("foo..bar", owned_path!("foo", OwnedSegment::Invalid)),
            ("[42]foo", owned_path!(42, "foo")),
            ("[-1]", owned_path!(-1)),
            ("[-42]", owned_path!(-42)),
            (".[-42]", owned_path!(-42)),
            ("[-42].foo", owned_path!(-42, "foo")),
            ("[-42]foo", owned_path!(-42, "foo")),
            (".\"[42]. {}-_\"", owned_path!("[42]. {}-_")),
            ("\"a\\\"a\"", owned_path!("a\"a")),
            (".\"a\\\"a\"", owned_path!("a\"a")),
            (
                ".foo.\"a\\\"a\".\"b\\\\b\".bar",
                owned_path!("foo", "a\"a", "b\\b", "bar"),
            ),
            (r#"."🤖""#, owned_path!("🤖")),
            (".(a|b)", owned_path!(vec!["a", "b"])),
            ("(a|b)", owned_path!(vec!["a", "b"])),
            ("( a | b )", owned_path!(vec!["a", "b"])),
            (".(a|b)[1]", owned_path!(vec!["a", "b"], 1)),
            (".(a|b).foo", owned_path!(vec!["a", "b"], "foo")),
            (".(a|b|c)", owned_path!(vec!["a", "b", "c"])),
            ("[1](a|b)", owned_path!(1, vec!["a", "b"])),
            ("[1].(a|b)", owned_path!(1, vec!["a", "b"])),
            ("foo.(a|b)", owned_path!("foo", vec!["a", "b"])),
            ("(\"a\"|b)", owned_path!(vec!["a", "b"])),
            ("(a|\"b.c\")", owned_path!(vec!["a", "b.c"])),
            ("(a|\"b\\\"c\")", owned_path!(vec!["a", "b\"c"])),
            ("(\"b\\\"c\"|a)", owned_path!(vec!["b\"c", "a"])),
            ("(a)", owned_path!(OwnedSegment::Invalid)),
        ];

        for (path, expected) in test_cases {
            if !Path::eq(&path, &expected) {
                panic!(
                    "Not equal. Input={:?}\nExpected: {:?}\nActual: {:?}",
                    path,
                    (&expected).segment_iter().collect::<Vec<_>>(),
                    path.segment_iter().collect::<Vec<_>>()
                );
            }
        }
    }
}
