use crate::event::Event;

pub mod parser;
pub mod query;

use string_cache::DefaultAtom as Atom;

pub type Result<T> = std::result::Result<T, String>;

pub trait Function: Send + core::fmt::Debug {
    fn apply(&self, target: &mut Event) -> Result<()>;
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Assignment {
    path: String,
    function: Box<dyn query::Function>,
}

impl Assignment {
    pub fn new(path: String, func: Box<dyn query::Function>) -> Self {
        Self {
            path: path,
            function: func,
        }
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
pub struct Deletion {
    // TODO: Switch to String once Event API is cleaned up.
    path: Atom,
}

impl Deletion {
    pub fn new(path: String) -> Self {
        Self {
            path: Atom::from(path),
        }
    }
}

impl Function for Deletion {
    fn apply(&self, target: &mut Event) -> Result<()> {
        target.as_mut_log().remove(&self.path);
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Mapping {
    assignments: Vec<Box<dyn Function>>,
}

impl Mapping {
    pub fn new(assignments: Vec<Box<dyn Function>>) -> Self {
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

#[cfg(test)]
mod test {
    use super::query::Literal;
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
                    event.as_mut_log().insert("foo", Value::from("bar"));
                    event
                },
                {
                    let mut event = Event::from("foo body");
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                Mapping::new(vec![Box::new(Deletion::new("foo".to_string()))]),
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
                    Box::new(Deletion::new("bar".to_string())),
                ]),
                Ok(()),
            ),
        ];

        for (mut input_event, exp_event, mapping, exp_result) in cases {
            assert_eq!(mapping.execute(&mut input_event), exp_result);
            assert_eq!(input_event, exp_event);
        }
    }
}
