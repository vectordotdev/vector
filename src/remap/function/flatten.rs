use remap::prelude::*;
use std::collections::btree_map;

#[derive(Clone, Copy, Debug)]
pub struct Flatten;

impl Function for Flatten {
    fn identifier(&self) -> &'static str {
        "flatten"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Array(_) | Value::Map(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        Ok(Box::new(FlattenFn { value }))
    }
}

#[derive(Debug, Clone)]
struct FlattenFn {
    value: Box<dyn Expression>,
}

impl FlattenFn {
    #[cfg(test)]
    pub fn new(value: Box<dyn Expression>) -> Self {
        FlattenFn { value }
    }
}

impl Expression for FlattenFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        match self.value.execute(state, object)? {
            Value::Array(arr) => Ok(Value::Array(
                ArrayFlatten::new(arr.iter()).cloned().collect(),
            )),
            Value::Map(map) => Ok(Value::Map(
                MapFlatten::new(map.iter())
                    .map(|(k, v)| (k, v.clone()))
                    .collect(),
            )),
            _ => unreachable!(),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Map | value::Kind::Array)
    }
}

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

#[cfg(test)]
mod test {
    use super::*;
    use crate::array;
    use crate::map;

    #[test]
    fn check_flatten() {
        let cases = vec![
            (
                map!["foo": Value::Array(array![42])],
                Ok(Value::Array(array![42])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": Value::Array(array![42, Value::Array(array![43, 44]),])],
                Ok(Value::from(vec![
                    Value::from(42),
                    Value::from(43),
                    Value::from(44),
                ])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": Value::Array(array![42, Value::Array(array![]), 43])],
                Ok(Value::Array(array![42, 43])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map![
                    "foo":
                        Value::Array(
                            array![
                                42,
                                Value::Array(array![43, 44, Value::Array(array![45, 46]),]),
                            ],
                        )
                ],
                Ok(Value::Array(array![42, 43, 44, 45, 46,])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map![
                    "foo":
                        Value::Array(
                            array![Value::Array(array![42, 43]), Value::Array(array![44, 45]),],
                        )
                ],
                Ok(Value::Array(array![42, 43, 44, 45,])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": map!["parent": "child"]],
                Ok(Value::from(map!["parent": "child"])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map![
                    "foo":
                        map!["parent": map![ "child1": 1,
                                                 "child2": 2],
                                 "key": "val"]
                ],
                Ok(Value::from(map!["parent.child1": 1,
                                    "parent.child2": 2,
                                    "key": "val"])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map![
                    "foo":
                        map![ "parent": map![ "child1": 1,
                                                       "child2": map![ "grandchild1": 1,
                                                                        "grandchild2": 2]],
                                       "key": "val"]
                ],
                Ok(Value::from(map!["parent.child1": 1,
                                    "parent.child2.grandchild1": 1,
                                    "parent.child2.grandchild2": 2,
                                    "key": "val"])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map![
                    "foo":
                        map!["parent": map![ "child1": Value::Array(array![1, Value::Array(array![2, 3])]),
                                                 "child2": map!["grandchild1": 1,
                                                                "grandchild2": Value::Array(array![1, Value::Array(array![2, 3]), 4])]],
                                 "key": "val"]
                ],
                Ok(Value::from(
                    map!["parent.child1": Value::Array(array![1, Value::Array(array![2, 3])]),
                                    "parent.child2.grandchild1": 1,
                                    "parent.child2.grandchild2": Value::Array(array![1, Value::Array(array![2, 3]), 4]),
                                    "key": "val"],
                )),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                // If the root object is an array, child maps are not flattened.
                map![
                    "foo":
                        Value::Array(
                            array![
                                map![
                                    "parent1":
                                        map!["child1": 1,
                                             "child2": 2]
                                ],
                                Value::Array(array![
                                    map![
                                        "parent2":
                                            map!["child3": 3,
                                                 "child4": 4]
                                    ],
                                    map!["parent3": map!["child5": 5]]
                                ])
                            ],
                        )
                ],
                Ok(Value::Array(array![
                    map![
                        "parent1":
                            map!["child1": 1,
                                             "child2": 2]
                    ],
                    map![
                        "parent2":
                            map!["child3": 3,
                                             "child4": 4]
                    ],
                    map!["parent3": map!["child5": 5]]
                ])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map![
                    "foo":
                        map!["parent1": map![ "child1": map![ "grandchild1": 1 ],
                                                   "child2": map![ "grandchild2": 2,
                                                                    "grandchild3": 3 ]],
                                 "parent2": 4]
                ],
                Ok(Value::from(map!["parent1.child1.grandchild1": 1,
                                    "parent1.child2.grandchild2": 2,
                                    "parent1.child2.grandchild3": 3,
                                    "parent2": 4])),
                FlattenFn::new(Box::new(Path::from("foo"))),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object = Value::Map(object);
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
