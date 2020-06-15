use super::{BuildError, Transform};
use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

use lazy_static::lazy_static;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ConcatConfig {
    pub target: Atom,
    pub joiner: Option<String>,
    pub items: Vec<Atom>,
}

inventory::submit! {
    TransformDescription::new_without_default::<ConcatConfig>("concat")
}

#[typetag::serde(name = "concat")]
impl TransformConfig for ConcatConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let joiner: String = match self.joiner.clone() {
            None => " ".into(),
            Some(var) => var.into(),
        };
        let items = self
            .items
            .iter()
            .map(|item| Substring::new(item))
            .collect::<Result<Vec<Substring>, BuildError>>()?;
        Ok(Box::new(Concat::new(self.target.clone(), joiner, items)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "concat"
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Substring {
    source: Atom,
    start: Option<i32>,
    end: Option<i32>,
}

impl Substring {
    fn new(input: &Atom) -> Result<Substring, BuildError> {
        lazy_static! {
            static ref SUBSTR_REGEX: Regex =
                Regex::new(r"^(?P<source>.*?)(?:\[(?P<start>-?[0-9]*)\.\.(?P<end>-?[0-9]*)\])?$")
                    .unwrap();
        }
        let cap = match SUBSTR_REGEX.captures(input.as_bytes()) {
            None => {
                return Err(BuildError::InvalidSubstring {
                    name: "invalid format, use 'source[start..end]' or 'source'".to_string(),
                })
            }
            Some(cap) => cap,
        };

        let source = match cap.name("source") {
            Some(source) => String::from_utf8_lossy(source.as_bytes()).into(),
            None => {
                return Err(BuildError::InvalidSubstring {
                    name: "invalid format, use 'source[start..end]' or 'source'".into(),
                })
            }
        };
        let start = match cap.name("start") {
            None => None,
            Some(var) => match String::from_utf8_lossy(var.as_bytes()).parse() {
                Ok(var) => Some(var),
                Err(_) => None,
            },
        };
        let end = match cap.name("end") {
            None => None,
            Some(var) => match String::from_utf8_lossy(var.as_bytes()).parse() {
                Ok(var) => Some(var),
                Err(_) => None,
            },
        };

        Ok(Self { source, start, end })
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Concat {
    target: Atom,
    joiner: String,
    items: Vec<Substring>,
}

impl Concat {
    pub fn new(target: Atom, joiner: String, items: Vec<Substring>) -> Self {
        Self {
            target,
            joiner,
            items,
        }
    }
}

impl Transform for Concat {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let mut content_vec: Vec<bytes::Bytes> = Vec::new();
        for substring in self.items.iter() {
            if let Some(value) = event.as_log().get(&substring.source) {
                let b = value.as_bytes();
                let start = match substring.start {
                    None => 0,
                    Some(s) => {
                        if s < 0 {
                            (b.len() as i32 + s) as usize
                        } else {
                            s as usize
                        }
                    }
                };
                let end = match substring.end {
                    None => b.len(),
                    Some(e) => {
                        if e < 0 {
                            (b.len() as i32 + e) as usize
                        } else {
                            e as usize
                        }
                    }
                };
                if start >= end {
                    error!(
                        "substring error on {}: start {} > end {}",
                        substring.source, start, end
                    );
                    return None;
                }
                if start > b.len() {
                    error!(
                        "substring error on {}: start {} > len {}",
                        substring.source,
                        start,
                        b.len()
                    );
                    return None;
                }
                if end > b.len() {
                    error!(
                        "substring error on {}: end {} > len {}",
                        substring.source,
                        end,
                        b.len()
                    );
                    return None;
                }
                content_vec.push(b.slice(start, end));
            }
        }

        let content = content_vec.join(self.joiner.as_bytes());
        event.as_mut_log().insert(self.target.clone(), content);

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::Concat;
    use super::Substring;
    use crate::{event::Event, transforms::Transform};

    #[test]
    fn concat_to_from() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("first", "Hello vector users");
        event.as_mut_log().insert("second", "World");

        let mut transform = Concat::new(
            "out".into(),
            " ".into(),
            vec![
                Substring::new(&"first[..5]".into()).unwrap(),
                Substring::new(&"first[-5..]".into()).unwrap(),
            ],
        );

        let new_event = transform.transform(event).unwrap();
        assert_eq!(new_event.as_log()[&"out".into()], "Hello users".into());
    }

    #[test]
    fn concat_full() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("first", "Hello vector users");
        event.as_mut_log().insert("second", "World");

        let mut transform = Concat::new(
            "out".into(),
            " ".into(),
            vec![
                Substring::new(&"first[..5]".into()).unwrap(),
                Substring::new(&"second".into()).unwrap(),
            ],
        );

        let new_event = transform.transform(event).unwrap();
        assert_eq!(new_event.as_log()[&"out".into()], "Hello World".into());
    }
    #[test]
    fn concat_mixed() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("first", "Hello vector users");
        event.as_mut_log().insert("second", "World");

        let mut transform = Concat::new(
            "out".into(),
            " ".into(),
            vec![
                Substring::new(&"second[..1]".into()).unwrap(),
                Substring::new(&"second[-4..2]".into()).unwrap(),
                Substring::new(&"second[-3..-2]".into()).unwrap(),
                Substring::new(&"second[3..-1]".into()).unwrap(),
                Substring::new(&"second[4..]".into()).unwrap(),
            ],
        );

        let new_event = transform.transform(event).unwrap();
        assert_eq!(new_event.as_log()[&"out".into()], "W o r l d".into());
    }

    #[test]
    fn concat_start_gt_end() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("only", "Hello vector users");

        let mut transform = Concat::new(
            "out".into(),
            " ".into(),
            vec![Substring::new(&"only[3..1]".into()).unwrap()],
        );

        assert!(transform.transform(event).is_none());
    }

    #[test]
    fn concat_start_gt_len() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("only", "World");

        let mut transform = Concat::new(
            "out".into(),
            " ".into(),
            vec![Substring::new(&"only[10..11]".into()).unwrap()],
        );

        assert!(transform.transform(event).is_none());
    }

    #[test]
    fn concat_end_gt_len() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("only", "World");

        let mut transform = Concat::new(
            "out".into(),
            " ".into(),
            vec![Substring::new(&"only[..11]".into()).unwrap()],
        );

        assert!(transform.transform(event).is_none());
    }
}
