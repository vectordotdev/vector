use crate::event::{Event, Value};
use std::collections::BTreeMap;

pub mod parser;
pub mod query;

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

/// Merges two BTreeMaps of `Value`s.
/// The second map is merged into the first one.
///
/// If `deep` is true, only the top level values are merged in. If both maps contain a field
/// with the same name, the field from the first is overwritten with the field from the second.
///
/// If `deep` is false, should both maps contain a field with the same name, and both those
/// fields are also maps, the function will recurse and will merge the child fields from the second
/// into the child fields from the first.
///
/// Note, this does recurse, so there is the theoretical possibility that it could blow up the
/// stack. From quick tests on a sample project I was able to merge maps with a depth of 3,500
/// before encountering issues. So I think that is likely to be within acceptable limits.
/// If it becomes a problem, we can unroll this function, but that will come at a cost of extra
/// code complexity.
fn merge_maps<K>(map1: &mut BTreeMap<K, Value>, map2: &BTreeMap<K, Value>, deep: bool)
where
    K: std::cmp::Ord + Clone,
{
    for (key2, value2) in map2.iter() {
        match (deep, map1.get_mut(key2), value2) {
            (true, Some(Value::Map(ref mut child1)), Value::Map(ref child2)) => {
                // We are doing a deep merge and both fields are maps.
                merge_maps(child1, child2, deep);
            }
            _ => {
                map1.insert(key2.clone(), value2.clone());
            }
        }
    }
}

#[derive(Debug)]
pub(in crate::mapping) struct MergeFn {
    to_path: Atom,
    from: Box<dyn query::Function>,
    deep: Option<Box<dyn query::Function>>,
}

impl MergeFn {
    pub(in crate::mapping) fn new(
        to_path: Atom,
        from: Box<dyn query::Function>,
        deep: Option<Box<dyn query::Function>>,
    ) -> Self {
        MergeFn {
            to_path,
            from,
            deep,
        }
    }
}

impl Function for MergeFn {
    fn apply(&self, target: &mut Event) -> Result<()> {
        let from_value = self.from.execute(target)?;
        let deep = match &self.deep {
            None => false,
            Some(deep) => match deep.execute(target)? {
                Value::Boolean(value) => value,
                _ => return Err("deep parameter passed to merge is a non-boolean value".into()),
            },
        };

        let to_value = target.as_mut_log().get_mut(&self.to_path).ok_or(format!(
            "parameter {} passed to merge is not found",
            self.to_path
        ))?;

        match (to_value, from_value) {
            (Value::Map(ref mut map1), Value::Map(ref map2)) => {
                merge_maps(map1, &map2, deep);
                Ok(())
            }

            _ => Err("parameters passed to merge are non-map values".into()),
        }
    }
}

//------------------------------------------------------------------------------

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

    #[test]
    fn check_merge() {
        let cases = vec![
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Boolean(true));
                    event
                        .as_mut_log()
                        .insert("bar", serde_json::json!({ "key2": "val2" }));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Boolean(true));
                    event
                        .as_mut_log()
                        .insert("bar", serde_json::json!({ "key2": "val2" }));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event
                },
                Mapping::new(vec![Box::new(MergeFn::new(
                    "foo".into(),
                    Box::new(QueryPath::from(vec![vec!["bar"]])),
                    None,
                ))]),
                Err(
                    "failed to apply mapping 0: parameters passed to merge are non-map values"
                        .into(),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", serde_json::json!({ "key1": "val1" }));
                    event
                        .as_mut_log()
                        .insert("bar", serde_json::json!({ "key2": "val2" }));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event.as_mut_log().remove(&Atom::from("message"));
                    event
                },
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", serde_json::json!({ "key1": "val1", "key2": "val2" }));
                    event
                        .as_mut_log()
                        .insert("bar", serde_json::json!({ "key2": "val2" }));
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event.as_mut_log().remove(&Atom::from("message"));
                    event
                },
                Mapping::new(vec![Box::new(MergeFn::new(
                    "foo".into(),
                    Box::new(QueryPath::from(vec![vec!["bar"]])),
                    None,
                ))]),
                Ok(()),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "parent1",
                        serde_json::json!(
                        { "key1": "val1",
                          "child": {
                              "grandchild1": "val1"
                          }
                        }),
                    );
                    event.as_mut_log().insert(
                        "parent2",
                        serde_json::json!(
                            { "key2": "val2",
                               "child": {
                                   "grandchild2": "val2"
                               }
                        }),
                    );
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event.as_mut_log().remove(&Atom::from("message"));
                    event
                },
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "parent1",
                        serde_json::json!(
                        { "key1": "val1",
                          "key2": "val2",
                          "child": {
                             "grandchild2": "val2"
                          }
                        }),
                    );
                    event.as_mut_log().insert(
                        "parent2",
                        serde_json::json!(
                            { "key2": "val2",
                              "child": {
                                  "grandchild2": "val2"
                              }
                        }),
                    );
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event.as_mut_log().remove(&Atom::from("message"));
                    event
                },
                Mapping::new(vec![Box::new(MergeFn::new(
                    "parent1".into(),
                    Box::new(QueryPath::from(vec![vec!["parent2"]])),
                    None,
                ))]),
                Ok(()),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "parent1",
                        serde_json::json!(
                        { "key1": "val1",
                          "child": {
                              "grandchild1": "val1"
                          }
                        }),
                    );
                    event.as_mut_log().insert(
                        "parent2",
                        serde_json::json!(
                            { "key2": "val2",
                               "child": {
                                   "grandchild2": "val2"
                               }
                        }),
                    );
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event.as_mut_log().remove(&Atom::from("message"));
                    event
                },
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "parent1",
                        serde_json::json!(
                        { "key1": "val1",
                          "key2": "val2",
                          "child": {
                              "grandchild1": "val1",
                              "grandchild2": "val2"
                          }
                        }),
                    );
                    event.as_mut_log().insert(
                        "parent2",
                        serde_json::json!(
                            { "key2": "val2",
                              "child": {
                                  "grandchild2": "val2"
                              }
                        }),
                    );
                    event.as_mut_log().remove(&Atom::from("timestamp"));
                    event.as_mut_log().remove(&Atom::from("message"));
                    event
                },
                Mapping::new(vec![Box::new(MergeFn::new(
                    "parent1".into(),
                    Box::new(QueryPath::from(vec![vec!["parent2"]])),
                    Some(Box::new(Literal::from(Value::Boolean(true)))),
                ))]),
                Ok(()),
            ),
        ];

        for (mut input_event, exp_event, mapping, exp_result) in cases {
            assert_eq!(mapping.execute(&mut input_event), exp_result);
            assert_eq!(input_event, exp_event);
        }
    }
}
