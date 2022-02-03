use std::collections::btree_map;

use vrl::prelude::*;

fn flatten(value: Value) -> Resolved {
    match value {
        Value::Array(arr) => Ok(Value::Array(
            ArrayFlatten::new(arr.iter()).cloned().collect(),
        )),
        Value::Object(map) => Ok(Value::Object(
            MapFlatten::new(map.iter())
                .map(|(k, v)| (k, v.clone()))
                .collect(),
        )),
        value => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::Array | Kind::Object,
        }
        .into()),
    }
}

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

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        Ok(Box::new(FlattenFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        flatten(value)
    }
}

#[derive(Debug, Clone)]
struct FlattenFn {
    value: Box<dyn Expression>,
}

impl Expression for FlattenFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        flatten(self.value.resolve(ctx)?)
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

#[cfg(test)]
mod test {
    use super::*;

    test_function![
        flatten => Flatten;

        array {
            args: func_args![value: value!([42])],
            want: Ok(value!([42])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }

        nested_array {
            args: func_args![value: value!([42, [43, 44]])],
            want: Ok(value!([42, 43, 44])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }

        nested_empty_array {
            args: func_args![value: value!([42, [], 43])],
            want: Ok(value!([42, 43])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }

        double_nested_array {
            args: func_args![value: value!([42, [43, 44, [45, 46]]])],
            want: Ok(value!([42, 43, 44, 45, 46])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }

        two_arrays {
            args: func_args![value: value!([[42, 43], [44, 45]])],
            want: Ok(value!([42, 43, 44, 45])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }

        map {
            args: func_args![value: value!({parent: "child"})],
            want: Ok(value!({parent: "child"})),
            tdef: TypeDef::new().object::<(), Kind>(map! { (): Kind::all() }),
        }

        nested_map {
            args: func_args![value: value!({parent: {child1: 1, child2: 2}, key: "val"})],
            want: Ok(value!({"parent.child1": 1, "parent.child2": 2, key: "val"})),
            tdef: TypeDef::new().object::<(), Kind>(map! { (): Kind::all() }),
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
            tdef: TypeDef::new().object::<(), Kind>(map! { (): Kind::all() }),
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
            tdef: TypeDef::new().object::<(), Kind>(map! { (): Kind::all() }),
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
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
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
            tdef: TypeDef::new().object::<(), Kind>(map! { (): Kind::all() }),
        }
    ];
}
