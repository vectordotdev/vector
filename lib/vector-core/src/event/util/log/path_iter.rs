use std::{borrow::Cow, mem, str::Chars};

use serde::{Deserialize, Serialize};
use substring::Substring;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum PathComponent<'a> {
    /// For example, in `a.b[0].c[2]` the keys are "a", "b", and "c".
    Key(Cow<'a, str>),
    /// For example, in `a.b[0].c[2]` the indexes are 0 and 2.
    Index(usize),
    /// Indicates that a parsing error occurred.
    Invalid,
}

impl<'a> PathComponent<'a> {
    pub fn into_static(self) -> PathComponent<'static> {
        match self {
            PathComponent::Key(k) => PathComponent::<'static>::Key(k.into_owned().into()),
            PathComponent::Index(u) => PathComponent::<'static>::Index(u),
            PathComponent::Invalid => PathComponent::Invalid,
        }
    }
}

/// Iterator over components of paths specified in form `a.b[0].c[2]`.
pub struct PathIter<'a> {
    path: &'a str,
    chars: Chars<'a>,
    state: State,
    temp: String,
    pos: usize,
}

impl<'a> PathIter<'a> {
    #[must_use]
    pub fn new(path: &'a str) -> PathIter {
        PathIter {
            path,
            chars: path.chars(),
            state: Default::default(),
            temp: String::default(),
            pos: 0,
        }
    }
}

enum State {
    Start,
    Key(usize),
    Escape,
    EscapedKey,
    Index(usize),
    Dot,
    OpeningBracket,
    ClosingBracket,
    End,
    Invalid,
}

impl Default for State {
    fn default() -> State {
        State::Start
    }
}

impl<'a> Iterator for PathIter<'a> {
    type Item = PathComponent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut res = None;
        loop {
            if let Some(res) = res {
                return res;
            }

            let c = self.chars.next();
            self.state = match mem::take(&mut self.state) {
                State::Start => match c {
                    Some('.') | Some('[') | Some(']') | None => State::Invalid,
                    Some('\\') => State::Escape,
                    Some(_) => State::Key(self.pos),
                },
                State::Key(start) => match c {
                    Some('.') | Some('[') | None => {
                        res = Some(Some(PathComponent::Key(
                            self.path.substring(start, self.pos).into(),
                        )));
                        char_to_state(c)
                    }
                    Some(']') => State::Invalid,
                    Some('\\') => {
                        self.temp.push_str(self.path.substring(start, self.pos));
                        State::Escape
                    }
                    Some(_) => State::Key(start),
                },
                State::EscapedKey => match c {
                    Some('.') | Some('[') | None => {
                        res = Some(Some(PathComponent::Key(
                            std::mem::take(&mut self.temp).into(),
                        )));
                        char_to_state(c)
                    }
                    Some(']') => State::Invalid,
                    Some('\\') => State::Escape,
                    Some(c) => {
                        self.temp.push(c);
                        State::EscapedKey
                    }
                },
                State::Escape => match c {
                    Some(c) if c == '.' || c == '[' || c == ']' || c == '\\' => {
                        self.temp.push(c);
                        State::EscapedKey
                    }
                    _ => State::Invalid,
                },
                State::Index(i) => match c {
                    Some(c) if ('0'..='9').contains(&c) => {
                        State::Index(10 * i + (c as usize - '0' as usize))
                    }
                    Some(']') => {
                        res = Some(Some(PathComponent::Index(i)));
                        State::ClosingBracket
                    }
                    _ => State::Invalid,
                },
                State::Dot => match c {
                    Some('.') | Some('[') | Some(']') | None => State::Invalid,
                    Some('\\') => State::Escape,
                    Some(_) => State::Key(self.pos),
                },
                State::OpeningBracket => match c {
                    Some(c) if ('0'..='9').contains(&c) => State::Index(c as usize - '0' as usize),
                    _ => State::Invalid,
                },
                State::ClosingBracket => match c {
                    Some('.') | Some('[') | None => char_to_state(c),
                    _ => State::Invalid,
                },
                State::End => {
                    res = Some(None);
                    State::End
                }
                State::Invalid => {
                    res = Some(Some(PathComponent::Invalid));
                    State::End
                }
            };
            self.pos += 1;
        }
    }
}

#[inline]
fn char_to_state(c: Option<char>) -> State {
    match c {
        Some('.') => State::Dot,
        Some('[') => State::OpeningBracket,
        Some(']') => State::ClosingBracket,
        Some('\\') => State::Escape,
        None => State::End,
        _ => State::Invalid,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn path_iter_elementary() {
        let actual: Vec<_> = PathIter::new("squirrel").collect();
        let expected = vec![PathComponent::Key("squirrel".into())];
        assert_eq!(actual, expected);
    }

    #[test]
    fn path_iter_complex() {
        use PathComponent::{Index, Key};

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

        for i in inputs {
            assert_eq!(PathIter::new(i).last(), Some(PathComponent::Invalid));
        }
    }
}
