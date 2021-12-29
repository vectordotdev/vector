use std::collections::btree_map;

use super::prelude::*;

/// An iterator to walk over maps allowing us to flatten nested maps to a single level.
struct MapFlatten<'a> {
    values: btree_map::Iter<'a, String, Value>,
    inner: Option<Box<MapFlatten<'a>>>,
    parent: Option<String>,
}

impl<'a> MapFlatten<'a> {
    fn new(values: btree_map::Iter<'a, String, Value>) -> Self {
        Self {
            values,
            inner: None,
            parent: None,
        }
    }

    fn new_from_parent(parent: String, values: btree_map::Iter<'a, String, Value>) -> Self {
        Self {
            values,
            inner: None,
            parent: Some(parent),
        }
    }

    /// Returns the key with the parent prepended.
    fn new_key(&self, key: &str) -> String {
        match self.parent {
            None => key.to_string(),
            Some(ref parent) => format!("{}.{}", parent, key),
        }
    }
}

impl<'a> std::iter::Iterator for MapFlatten<'a> {
    type Item = (String, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ref mut inner) = self.inner {
            let next = inner.next();
            match next {
                Some(_) => return next,
                None => self.inner = None,
            }
        }

        let next = self.values.next();
        match next {
            Some((key, Value::Map(value))) => {
                self.inner = Some(Box::new(MapFlatten::new_from_parent(
                    self.new_key(key),
                    value.iter(),
                )));
                self.next()
            }
            Some((key, value)) => Some((self.new_key(key), value)),
            None => None,
        }
    }
}

/// Create an iterator that can walk a tree of Array values.
/// This can be used to flatten the array.
struct ArrayFlatten<'a> {
    values: std::slice::Iter<'a, Value>,
    inner: Option<Box<ArrayFlatten<'a>>>,
}

impl<'a> ArrayFlatten<'a> {
    fn new(values: std::slice::Iter<'a, Value>) -> Self {
        ArrayFlatten {
            values,
            inner: None,
        }
    }
}

impl<'a> std::iter::Iterator for ArrayFlatten<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        // Iterate over our inner list first.
        if let Some(ref mut inner) = self.inner {
            let next = inner.next();
            match next {
                Some(_) => return next,
                None => {
                    // The inner list has been exhausted.
                    self.inner = None;
                }
            }
        }

        // Then iterate over our values.
        let next = self.values.next();
        match next {
            Some(Value::Array(next)) => {
                // Create a new iterator for this child list.
                self.inner = Some(Box::new(ArrayFlatten::new(next.iter())));
                self.next()
            }
            _ => next,
        }
    }
}

#[derive(Debug)]
pub(in crate::mapping) struct FlattenFn {
    value: Box<dyn Function>,
}

impl FlattenFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(value: Box<dyn Function>) -> Self {
        FlattenFn { value }
    }
}

impl Function for FlattenFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let value = required_value!(ctx, self.value,
        Value::Array(arr) => Value::Array(
            ArrayFlatten::new(arr.iter()).cloned().collect()
        ),
        Value::Map(map) => Value::Map(
            MapFlatten::new(map.iter()).map(|(k, v)| (k, v.clone())).collect()
        ));

        Ok(value.into())
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| {
                matches!(
                    v,
                    QueryValue::Value(Value::Array(_)) | QueryValue::Value(Value::Map(_))
                )
            },
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for FlattenFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let value = arguments.required("value")?;
        Ok(Self { value })
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn check_flatten() {
        let cases = vec![
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from(vec![Value::from(42)]));
                    event
                },
                Ok(Value::from(vec![Value::from(42)])),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::from(vec![
                            Value::from(42),
                            Value::from(vec![Value::from(43), Value::from(44)]),
                        ]),
                    );
                    event
                },
                Ok(Value::from(vec![
                    Value::from(42),
                    Value::from(43),
                    Value::from(44),
                ])),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::from(vec![Value::from(42), Value::Array(vec![]), Value::from(43)]),
                    );
                    event
                },
                Ok(Value::from(vec![Value::from(42), Value::from(43)])),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::from(vec![
                            Value::from(42),
                            Value::from(vec![
                                Value::from(43),
                                Value::from(44),
                                Value::from(vec![Value::from(45), Value::from(46)]),
                            ]),
                        ]),
                    );
                    event
                },
                Ok(Value::from(vec![
                    Value::from(42),
                    Value::from(43),
                    Value::from(44),
                    Value::from(45),
                    Value::from(46),
                ])),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::from(vec![
                            Value::from(vec![Value::from(42), Value::from(43)]),
                            Value::from(vec![Value::from(44), Value::from(45)]),
                        ]),
                    );
                    event
                },
                Ok(Value::from(vec![
                    Value::from(42),
                    Value::from(43),
                    Value::from(44),
                    Value::from(45),
                ])),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    let map = json!({"parent": "child"});
                    event.as_mut_log().insert("foo", Value::from(map));
                    event
                },
                Ok(Value::from(json!({"parent": "child"}))),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    let map = json!({"parent": { "child1": 1,
                                                 "child2": 2},
                                     "key": "val"});
                    event.as_mut_log().insert("foo", Value::from(map));
                    event
                },
                Ok(Value::from(json!({"parent.child1": 1,
                                      "parent.child2": 2,
                                      "key": "val"}))),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    let map = json!({"parent": { "child1": 1,
                                                 "child2": { "grandchild1": 1,
                                                             "grandchild2": 2}},
                                     "key": "val"});
                    event.as_mut_log().insert("foo", Value::from(map));
                    event
                },
                Ok(Value::from(json!({"parent.child1": 1,
                                      "parent.child2.grandchild1": 1,
                                      "parent.child2.grandchild2": 2,
                                      "key": "val"}))),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                // If the root object is a map, child arrays are not flattened.
                {
                    let mut event = Event::from("");
                    let map = json!({"parent": { "child1": [1, [2, 3]],
                                                 "child2": { "grandchild1": 1,
                                                             "grandchild2": [1, [2, 3], 4]}},
                                     "key": "val"});
                    event.as_mut_log().insert("foo", Value::from(map));
                    event
                },
                Ok(Value::from(json!({"parent.child1": [1, [2, 3]],
                                      "parent.child2.grandchild1": 1,
                                      "parent.child2.grandchild2": [1, [2, 3], 4],
                                      "key": "val"}))),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                // If the root object is an array, child maps are not flattened.
                {
                    let mut event = Event::from("");
                    let map = json!(
                        [
                            {"parent1": {"child1": 1,
                                         "child2": 2}},
                            [
                                {"parent2": {"child3": 3,
                                             "child4": 4}},
                                {"parent3": {"child5": 5}}
                            ]
                        ]
                    );
                    event.as_mut_log().insert("foo", Value::from(map));
                    event
                },
                Ok(Value::from(json!(
                    [
                        {"parent1": {"child1": 1,
                                     "child2": 2}},
                        {"parent2": {"child3": 3,
                                     "child4": 4}},
                        {"parent3": {"child5": 5}}
                    ]
                ))),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    let map = json!(
                        {"parent1": { "child1": { "grandchild1": 1 },
                                      "child2": { "grandchild2": 2,
                                                  "grandchild3": 3 }
                        },
                         "parent2": 4}
                    );
                    event.as_mut_log().insert("foo", Value::from(map));
                    event
                },
                Ok(Value::from(json!({"parent1.child1.grandchild1": 1,
                                      "parent1.child2.grandchild2": 2,
                                      "parent1.child2.grandchild3": 3,
                                      "parent2": 4}))),
                FlattenFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
