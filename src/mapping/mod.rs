use crate::event::{Event, Value};

pub mod parser;
pub mod query;

use bytes::{Bytes, BytesMut};
use string_cache::DefaultAtom as Atom;

pub type Result<T> = std::result::Result<T, String>;

pub(self) trait Function: Send + core::fmt::Debug {
    fn apply(&self, target: &mut Event) -> Result<()>;
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct Assignment {
    path: String,
    function: Box<dyn query::Function>,
}

impl Assignment {
    pub(self) fn new(path: String, function: Box<dyn query::Function>) -> Self {
        Self { path, function }
    }
}

impl Function for Assignment {
    fn apply(&self, target: &mut Event) -> Result<()> {
        let v = self.function.execute(&target)?;
        target.as_mut_log().insert(&self.path, v);
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct Deletion {
    // TODO: Switch to String once Event API is cleaned up.
    paths: Vec<Atom>,
}

impl Deletion {
    pub(self) fn new(mut paths: Vec<String>) -> Self {
        Self {
            paths: paths.drain(..).map(Atom::from).collect(),
        }
    }
}

impl Function for Deletion {
    fn apply(&self, target: &mut Event) -> Result<()> {
        for path in &self.paths {
            target.as_mut_log().remove(&path);
        }
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct OnlyFields {
    paths: Vec<String>,
}

impl OnlyFields {
    pub(self) fn new(paths: Vec<String>) -> Self {
        Self { paths }
    }
}

impl Function for OnlyFields {
    fn apply(&self, target: &mut Event) -> Result<()> {
        let target_log = target.as_mut_log();

        let keys: Vec<String> = target_log
            .keys()
            .filter(|k| {
                self.paths
                    .iter()
                    .find(|p| k.starts_with(p.as_str()))
                    .is_none()
            })
            .collect();

        for key in keys {
            target_log.remove_prune(&Atom::from(key), true);
        }

        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct Upcase {
    // TODO: Switch to String once Event API is cleaned up.
    paths: Vec<Atom>,
}

impl Upcase {
    pub(self) fn new(mut paths: Vec<String>) -> Self {
        Self {
            paths: paths.drain(..).map(Atom::from).collect(),
        }
    }
}

impl Function for Upcase {
    fn apply(&self, target: &mut Event) -> Result<()> {
        let target_log = target.as_mut_log();

        for path in &self.paths {
            mutate_bytes(target_log.get_mut(path), |mut buf| {
                buf.iter_mut().for_each(|c| c.make_ascii_uppercase());
                buf.freeze()
            })
        }
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct Downcase {
    // TODO: Switch to String once Event API is cleaned up.
    paths: Vec<Atom>,
}

impl Downcase {
    pub(self) fn new(mut paths: Vec<String>) -> Self {
        Self {
            paths: paths.drain(..).map(Atom::from).collect(),
        }
    }
}

impl Function for Downcase {
    fn apply(&self, target: &mut Event) -> Result<()> {
        let target_log = target.as_mut_log();

        for path in &self.paths {
            mutate_bytes(target_log.get_mut(path), |mut buf| {
                buf.iter_mut().for_each(|c| c.make_ascii_lowercase());
                buf.freeze()
            })
        }
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct IfStatement {
    query: Box<dyn query::Function>,
    true_statement: Box<dyn Function>,
    false_statement: Box<dyn Function>,
}

impl IfStatement {
    pub(self) fn new(
        query: Box<dyn query::Function>,
        true_statement: Box<dyn Function>,
        false_statement: Box<dyn Function>,
    ) -> Self {
        Self {
            query,
            true_statement,
            false_statement,
        }
    }
}

impl Function for IfStatement {
    fn apply(&self, target: &mut Event) -> Result<()> {
        match self.query.execute(target)? {
            Value::Boolean(true) => self.true_statement.apply(target),
            Value::Boolean(false) => self.false_statement.apply(target),
            _ => Err("query returned non-boolean value".to_string()),
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct Noop {}

impl Function for Noop {
    fn apply(&self, _: &mut Event) -> Result<()> {
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Mapping {
    assignments: Vec<Box<dyn Function>>,
}

impl Mapping {
    pub(self) fn new(assignments: Vec<Box<dyn Function>>) -> Self {
        Mapping { assignments }
    }

    pub fn execute(&self, event: &mut Event) -> Result<()> {
        for (i, assignment) in self.assignments.iter().enumerate() {
            if let Err(err) = assignment.apply(event) {
                return Err(format!("failed to apply mapping {}: {}", i, err));
            }
        }
        Ok(())
    }
}

//------------------------------------------------------------------------------

fn mutate_bytes<F>(value: Option<&mut Value>, f: F)
where
    F: Fn(BytesMut) -> Bytes,
{
    if let Some(value) = value {
        if let Value::Bytes(ref mut bytes) = value {
            let mut buf = BytesMut::with_capacity(bytes.len());
            buf.extend_from_slice(bytes);
            *bytes = f(buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::query::{
        arithmetic::Arithmetic, arithmetic::Operator as ArithmeticOperator,
        path::Path as QueryPath, Literal,
    };
    use super::*;
    use crate::event::{Event, Value};

    #[test]
    fn check_mapping() {
        let cases = vec![
            (
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().insert("foo", Value::from("bar"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Literal::from(Value::from("bar"))),
                ))]),
                Ok(()),
            ),
            (
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                {
                    let mut event = Event::from("foo body");
                    event
                        .as_mut_log()
                        .insert("foo bar\\.baz.buz", Value::from("quack"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo bar\\.baz.buz".to_string(),
                    Box::new(Literal::from(Value::from("quack"))),
                ))]),
                Ok(()),
            ),
            (
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event.as_mut_log().insert("foo", Value::from("bar"));
                    event
                },
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                Mapping::new(vec![Box::new(Deletion::new(vec!["foo".to_string()]))]),
                Ok(()),
            ),
            (
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().insert("bar", Value::from("baz"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().insert("foo", Value::from("bar"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                Mapping::new(vec![
                    Box::new(Assignment::new(
                        "foo".to_string(),
                        Box::new(Literal::from(Value::from("bar"))),
                    )),
                    Box::new(Deletion::new(vec!["bar".to_string()])),
                ]),
                Ok(()),
            ),
            (
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().insert("bar", Value::from("baz"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().insert("bar", Value::from("baz"));
                    event.as_mut_log().insert("foo", Value::from("bar is baz"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                Mapping::new(vec![Box::new(IfStatement::new(
                    Box::new(Arithmetic::new(
                        Box::new(QueryPath::from("bar")),
                        Box::new(Literal::from(Value::from("baz"))),
                        ArithmeticOperator::Equal,
                    )),
                    Box::new(Assignment::new(
                        "foo".to_string(),
                        Box::new(Literal::from(Value::from("bar is baz"))),
                    )),
                    Box::new(Deletion::new(vec!["bar".to_string()])),
                ))]),
                Ok(()),
            ),
            (
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().insert("bar", Value::from("buz"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                Mapping::new(vec![Box::new(IfStatement::new(
                    Box::new(Arithmetic::new(
                        Box::new(QueryPath::from("bar")),
                        Box::new(Literal::from(Value::from("baz"))),
                        ArithmeticOperator::Equal,
                    )),
                    Box::new(Assignment::new(
                        "foo".to_string(),
                        Box::new(Literal::from(Value::from("bar is baz"))),
                    )),
                    Box::new(Deletion::new(vec!["bar".to_string()])),
                ))]),
                Ok(()),
            ),
            (
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().insert("bar", Value::from("buz"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().insert("bar", Value::from("buz"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                Mapping::new(vec![Box::new(IfStatement::new(
                    Box::new(QueryPath::from("bar")),
                    Box::new(Assignment::new(
                        "foo".to_string(),
                        Box::new(Literal::from(Value::from("bar is baz"))),
                    )),
                    Box::new(Deletion::new(vec!["bar".to_string()])),
                ))]),
                Err("failed to apply mapping 0: query returned non-boolean value".to_string()),
            ),
            (
                {
                    let mut event = Event::from("foo body");
                    event
                        .as_mut_log()
                        .insert("bar.baz.buz", Value::from("first"));
                    event
                        .as_mut_log()
                        .insert("bar.baz.remove_this", Value::from("second"));
                    event.as_mut_log().insert("bev", Value::from("third"));
                    event
                        .as_mut_log()
                        .insert("and.remove_this", Value::from("fourth"));
                    event
                        .as_mut_log()
                        .insert("nested.stuff.here", Value::from("fifth"));
                    event
                        .as_mut_log()
                        .insert("nested.and_here", Value::from("sixth"));
                    event
                },
                {
                    let mut event = Event::from("foo body");
                    event
                        .as_mut_log()
                        .insert("bar.baz.buz", Value::from("first"));
                    event.as_mut_log().insert("bev", Value::from("third"));
                    event
                        .as_mut_log()
                        .insert("nested.stuff.here", Value::from("fifth"));
                    event
                        .as_mut_log()
                        .insert("nested.and_here", Value::from("sixth"));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event.as_mut_log().remove(&Atom::from("message"));
                    event
                },
                Mapping::new(vec![Box::new(OnlyFields::new(vec![
                    "bar.baz.buz".to_string(),
                    "bev".to_string(),
                    "doesnt_exist.anyway".to_string(),
                    "nested".to_string(),
                ]))]),
                Ok(()),
            ),
        ];

        for (mut input_event, exp_event, mapping, exp_result) in cases {
            assert_eq!(mapping.execute(&mut input_event), exp_result);
            assert_eq!(input_event, exp_event);
        }
    }
}
