use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{mem, str::Chars};

lazy_static! {
    static ref FAST_RE: Regex = Regex::new(r"\A\w+(\.\w+)*\z").unwrap();
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum PathComponent {
    /// For example, in `a.b[0].c[2]` the keys are "a", "b", and "c".
    Key(String),
    /// For example, in `a.b[0].c[2]` the indexes are 0 and 2.
    Index(usize),
    /// Indicates that a parsing error occurred.
    Invalid,
}

/// Iterator over components of paths specified in form `a.b[0].c[2]`.
pub struct PathIter<'a> {
    path: &'a str,
    chars: Chars<'a>,
    state: PathIterState<'a>,
}

impl<'a> PathIter<'a> {
    pub fn new(path: &'a str) -> PathIter {
        PathIter {
            path,
            chars: path.chars(),
            state: Default::default(),
        }
    }
}

/// The parsing is implemented using a state machine.
/// The idea of using Rust enums to model states is taken from
/// https://hoverbear.org/blog/rust-state-machine-pattern/ .
enum PathIterState<'a> {
    Start,
    Fast(std::str::Split<'a, char>),
    Key(String),
    KeyEscape(String), // escape mode inside keys entered into after `\` character
    Index(usize),
    Dot,
    OpeningBracket,
    ClosingBracket,
    End,
    Invalid,
}

impl PathIterState<'_> {
    fn is_start(&self) -> bool {
        matches!(self, Self::Start)
    }
}

impl<'a> Default for PathIterState<'a> {
    fn default() -> PathIterState<'a> {
        PathIterState::Start
    }
}

impl<'a> Iterator for PathIter<'a> {
    type Item = PathComponent;

    fn next(&mut self) -> Option<Self::Item> {
        use PathIterState::*;

        if self.state.is_start() && FAST_RE.is_match(self.path) {
            self.state = Fast(self.path.split('.'));
        }

        let mut res = None;
        loop {
            if let Some(res) = res {
                return res;
            }

            let c = self.chars.next();
            self.state = match mem::take(&mut self.state) {
                Start => match c {
                    Some('.') | Some('[') | Some(']') | None => Invalid,
                    Some('\\') => KeyEscape(String::new()),
                    Some(c) => Key(c.to_string()),
                },
                Key(mut s) => match c {
                    Some('.') => {
                        res = Some(Some(PathComponent::Key(s)));
                        Dot
                    }
                    Some('[') => {
                        res = Some(Some(PathComponent::Key(s)));
                        OpeningBracket
                    }
                    Some(']') => Invalid,
                    Some('\\') => KeyEscape(s),
                    None => {
                        res = Some(Some(PathComponent::Key(s)));
                        End
                    }
                    Some(c) => {
                        s.push(c);
                        Key(s)
                    }
                },
                KeyEscape(mut s) => match c {
                    Some(c) if c == '.' || c == '[' || c == ']' || c == '\\' => {
                        s.push(c);
                        Key(s)
                    }
                    _ => Invalid,
                },
                Index(i) => match c {
                    Some(c) if ('0'..='9').contains(&c) => {
                        Index(10 * i + (c as usize - '0' as usize))
                    }
                    Some(']') => {
                        res = Some(Some(PathComponent::Index(i)));
                        ClosingBracket
                    }
                    _ => Invalid,
                },
                Dot => match c {
                    Some('.') | Some('[') | Some(']') | None => Invalid,
                    Some('\\') => KeyEscape(String::new()),
                    Some(c) => Key(c.to_string()),
                },
                OpeningBracket => match c {
                    Some(c) if ('0'..='9').contains(&c) => Index(c as usize - '0' as usize),
                    _ => Invalid,
                },
                ClosingBracket => match c {
                    Some('.') => Dot,
                    Some('[') => OpeningBracket,
                    None => End,
                    _ => Invalid,
                },
                End => {
                    res = Some(None);
                    End
                }
                Invalid => {
                    res = Some(Some(PathComponent::Invalid));
                    End
                }
                Fast(mut iter) => {
                    res = Some(iter.next().map(|s| PathComponent::Key(s.to_string())));
                    Fast(iter)
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn path_iter_elementary() {
        let actual: Vec<_> = PathIter::new(&"squirrel".to_string()).collect();
        let expected = vec![PathComponent::Key("squirrel".into())];
        assert_eq!(actual, expected);
    }

    #[test]
    fn path_iter_complex() {
        use PathComponent::*;

        let inputs = vec![
            "flying.squirrels.are.everywhere",
            "flying.squirrel[137][0].tail",
            "flying[0].squirrel[1]",
            "flying\\[0\\]\\.squirrel[1].\\\\tail\\\\",
        ];

        let expected = vec![
            vec![
                Key("flying".into()),
                Key("squirrels".into()),
                Key("are".into()),
                Key("everywhere".into()),
            ],
            vec![
                Key("flying".into()),
                Key("squirrel".into()),
                Index(137),
                Index(0),
                Key("tail".into()),
            ],
            vec![
                Key("flying".into()),
                Index(0),
                Key("squirrel".into()),
                Index(1),
            ],
            vec![
                Key("flying[0].squirrel".into()),
                Index(1),
                Key("\\tail\\".into()),
            ],
        ];

        for (i, e) in inputs.into_iter().zip(expected) {
            assert_eq!(PathIter::new(i).collect::<Vec<_>>(), e);
        }
    }

    #[test]
    fn path_iter_invalid() {
        let inputs = vec![
            "fly[asdf]",
            "flying..flying",
            "flying[0]]",
            "[0]",
            ".",
            ".flying[0]",
            "",
            "invalid\\ escaping",
        ];

        for i in inputs.into_iter() {
            assert_eq!(PathIter::new(i).last(), Some(PathComponent::Invalid));
        }
    }
}
