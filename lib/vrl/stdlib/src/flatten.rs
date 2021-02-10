use std::collections::btree_map;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Flatten;

impl Function for Flatten {
    fn identifier(&self) -> &'static str {
        "flatten"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::OBJECT | kind::ARRAY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "object",
                source: r#"flatten({ "foo": { "bar": true }})"#,
                result: Ok(r#"{ "foo.bar": true }"#),
            },
            Example {
                title: "array",
                source: r#"flatten([[true]])"#,
                result: Ok(r#"[true]"#),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        Ok(Box::new(FlattenFn { value }))
    }
}

#[derive(Debug, Clone)]
struct FlattenFn {
    value: Box<dyn Expression>,
}

impl Expression for FlattenFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        match self.value.resolve(ctx)? {
            Value::Array(arr) => Ok(Value::Array(
                ArrayFlatten::new(arr.iter()).cloned().collect(),
            )),
            Value::Object(map) => Ok(Value::Object(
                MapFlatten::new(map.iter())
                    .map(|(k, v)| (k, v.clone()))
                    .collect(),
            )),
            _ => unreachable!(),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let td = self.value.type_def(state);

        if td.is_array() {
            TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() })
        } else {
            TypeDef::new().object::<(), Kind>(map! { (): Kind::all() })
        }
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
            Some((key, Value::Object(value))) => {
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

/*
#[cfg(test)]
mod test {
    use super::*;

    test_function![
        flatten => Flatten;

        array {
            args: func_args![value: value!([42])],
            want: Ok(value!([42])),
        }

        nested_array {
            args: func_args![value: value!([42, [43, 44]])],
            want: Ok(value!([42, 43, 44])),
        }

        nested_empty_array {
            args: func_args![value: value!([42, [], 43])],
            want: Ok(value!([42, 43])),
        }

        double_nested_array {
            args: func_args![value: value!([42, [43, 44, [45, 46]]])],
            want: Ok(value!([42, 43, 44, 45, 46])),
        }

        two_arrays {
            args: func_args![value: value!([[42, 43], [44, 45]])],
            want: Ok(value!([42, 43, 44, 45])),
        }

        map {
            args: func_args![value: value!({parent: "child"})],
            want: Ok(value!({parent: "child"})),
        }

        nested_map {
            args: func_args![value: value!({parent: {child1: 1, child2: 2}, key: "val"})],
            want: Ok(value!({"parent.child1": 1, "parent.child2": 2, key: "val"})),
        }

        double_nested_map {
            args: func_args![value: value!({
                parent: {
                    child1: 1,
                    child2: { grandchild1: 1, grandchild2: 2 },
                },
                key: "val",
            })],
            want: Ok(value!({
                "parent.child1": 1,
                "parent.child2.grandchild1": 1,
                "parent.child2.grandchild2": 2,
                key: "val",
            })),
        }

        map_and_array {
            args: func_args![value: value!({
                parent: {
                    child1: [1, [2, 3]],
                    child2: {grandchild1: 1, grandchild2: [1, [2, 3], 4]},
                },
                key: "val",
            })],
            want: Ok(value!({
                "parent.child1": [1, [2, 3]],
                "parent.child2.grandchild1": 1,
                "parent.child2.grandchild2": [1, [2, 3], 4],
                key: "val",
            })),
        }

        // If the root object is an array, child maps are not flattened.
        root_array {
            args: func_args![value: value!([
                { parent1: { child1: 1, child2: 2 } },
                [
                    { parent2: { child3: 3, child4: 4 } },
                    { parent3: { child5: 5 } },
                ],
            ])],
            want: Ok(value!([
                { parent1: { child1: 1, child2: 2 } },
                { parent2: { child3: 3, child4: 4 } },
                { parent3: { child5: 5 } },
            ])),
        }

        triple_nested_map {
            args: func_args![value: value!({
                parent1: {
                    child1: { grandchild1: 1 },
                    child2: { grandchild2: 2, grandchild3: 3 },
                },
                parent2: 4,
            })],
            want: Ok(value!({
                "parent1.child1.grandchild1": 1,
                "parent1.child2.grandchild2": 2,
                "parent1.child2.grandchild3": 3,
                parent2: 4,
            })),
        }
    ];
}
*/
