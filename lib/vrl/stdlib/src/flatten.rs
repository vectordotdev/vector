use std::collections::BTreeMap;
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

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
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
        let value = self.value.resolve(ctx)?;
        let value = value.borrow();
        match &*value {
            Value::Array(arr) => Ok(SharedValue::from(Value::Array(flatten_array(arr)))),
            Value::Object(map) => Ok(SharedValue::from(Value::Object(flatten_object(None, map)))),
            value => Err(value::Error::Expected {
                got: value.kind(),
                expected: Kind::Array | Kind::Object,
            }
            .into()),
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

fn flatten_array(arr: &[SharedValue]) -> Vec<SharedValue> {
    let mut result = Vec::new();

    for value in arr {
        let borrowed = value.borrow();
        match &*borrowed {
            Value::Array(inner) => result.append(&mut flatten_array(inner)),
            _ => result.push(value.clone()),
        }
    }

    result
}

/// Returns the key with the parent prepended.
fn new_key(parent: Option<&str>, key: &str) -> String {
    match parent {
        None => key.to_string(),
        Some(ref parent) => format!("{}.{}", parent, key),
    }
}

fn flatten_object(
    parent: Option<String>,
    obj: &BTreeMap<String, SharedValue>,
) -> BTreeMap<String, SharedValue> {
    let mut result = BTreeMap::new();

    for (key, value) in obj {
        let key = new_key(parent.as_ref().map(|key| key.as_ref()), key);
        let borrowed = value.borrow();
        match &*borrowed {
            Value::Object(inner) => result.append(&mut flatten_object(Some(key), inner)),
            _ => {
                result.insert(key, value.clone());
            }
        }
    }

    result
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
