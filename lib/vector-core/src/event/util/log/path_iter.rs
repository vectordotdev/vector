use serde::{Deserialize, Serialize};
use std::{borrow::Cow, mem, str::Chars};
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
        use State::{
            ClosingBracket, Dot, End, Escape, EscapedKey, Index, Invalid, Key, OpeningBracket,
            Start,
        };

        let mut res = None;
        loop {
            if let Some(res) = res {
                return res;
            }

            let c = self.chars.next();
            self.state = match mem::take(&mut self.state) {
                Start => match c {
                    Some('.') | Some('[') | Some(']') | None => Invalid,
                    Some('\\') => Escape,
                    Some(_) => Key(self.pos),
                },
                Key(start) => match c {
                    Some('.') => {
                        res = Some(Some(PathComponent::Key(
                            self.path.substring(start, self.pos).into(),
                        )));
                        Dot
                    }
                    Some('[') => {
                        res = Some(Some(PathComponent::Key(
                            self.path.substring(start, self.pos).into(),
                        )));
                        OpeningBracket
                    }
                    Some(']') => Invalid,
                    Some('\\') => {
                        self.temp.push_str(self.path.substring(start, self.pos));
                        Escape
                    }
                    Some(_) => Key(start),
                    None => {
                        res = Some(Some(PathComponent::Key(
                            self.path.substring(start, self.pos).into(),
                        )));
                        End
                    }
                },
                EscapedKey => match c {
                    Some('.') => {
                        res = Some(Some(PathComponent::Key(
                            std::mem::take(&mut self.temp).into(),
                        )));
                        Dot
                    }
                    Some('[') => {
                        res = Some(Some(PathComponent::Key(
                            std::mem::take(&mut self.temp).into(),
                        )));
                        OpeningBracket
                    }
                    Some(']') => Invalid,
                    Some('\\') => Escape,
                    Some(c) => {
                        self.temp.push(c);
                        EscapedKey
                    }
                    None => {
                        res = Some(Some(PathComponent::Key(
                            std::mem::take(&mut self.temp).into(),
                        )));
                        End
                    }
                },
                Escape => match c {
                    Some(c) if c == '.' || c == '[' || c == ']' || c == '\\' => {
                        self.temp.push(c);
                        EscapedKey
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
                    Some('\\') => Escape,
                    Some(_) => Key(self.pos),
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
            };
            self.pos += 1;
        }
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
