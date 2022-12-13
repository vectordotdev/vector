use std::collections::{btree_map::Entry, BTreeMap};

use ::value::Value;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn zip(keys: Value, items: Value) -> Resolved {
    let keys = keys.try_array()?;
    let items = items.try_array()?;
    let mut map = BTreeMap::new();

    // iterate both arrays, inserting {k: v} per index to new obj
    for (key, value) in keys.iter().zip(items.into_iter()) {
        let key = key.try_bytes_utf8_lossy()?;
        match map.entry(key.into_owned()) {
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
            Entry::Occupied(mut entry) => {
                // always replace previous entry
                let existing = entry.get_mut();
                *existing = value;
            }
        }
    }
    Ok(Value::Object(map))
}

#[derive(Clone, Copy, Debug)]
pub struct Zip;

impl Function for Zip {
    fn identifier(&self) -> &'static str {
        "zip"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "keys",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "items",
                kind: kind::ARRAY,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "zip arrays into an object (first must be strings)",
                source: r#"zip(["a", "b"], [1, 2])"#,
                result: Ok("{\"a\": 1, \"b\": 2}"),
            },
            Example {
                title: "zip arrays of strings",
                source: r#"zip(["a", "b"], ["c", "d"])"#,
                result: Ok("{\"a\": \"c\", \"b\": \"d\"}"),
            },
            Example {
                title: "zip duplicate keys (last one wins)",
                source: r#"zip(["a", "a"], ["c", "d"])"#,
                result: Ok("{\"a\": \"d\"}"),
            },
            Example {
                title: "odd length arrays iterate to shortest length",
                source: r#"zip(["a", "b"], [1])"#,
                result: Ok("{\"a\": 1}"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let keys = arguments.required("keys");
        let items = arguments.required("items");

        Ok(ZipFn { keys, items }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct ZipFn {
    keys: Box<dyn Expression>,
    items: Box<dyn Expression>,
}

impl FunctionExpression for ZipFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let keys = self.keys.resolve(ctx)?;
        let items = self.items.resolve(ctx)?;

        zip(keys, items)
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        let keys = self.keys.type_def(state).restrict_array();
        let items = self.items.type_def(state).restrict_array();

        let _ = keys.as_array().expect("must be an array");
        let _ = items.as_array().expect("must be an array");

        TypeDef::object(Collection::any()).infallible()
    }
}

#[cfg(test)]
mod tests {
    // use vector_common::btreemap;
    // use vrl::value::Kind;

    use super::*;

    test_function![
        zip => Zip;

        both_arrays_empty {
            args: func_args![
                keys: value!([]),
                items: value!([])
            ],
            want: Ok(value!({})),
            tdef: TypeDef::object(Collection::any()),
        }

        both_arrays_full {
            args: func_args![
                keys: value!(["a", "b"]),
                items: value!(["c", "d"])
            ],
            want: Ok(value!({ "a": "c", "b": "d" })),
            tdef: TypeDef::object(Collection::any()),
        }

        repeated_key {
            args: func_args![
                keys: value!(["a", "a"]),
                items: value!([1, 2])
            ],
            want: Ok(value!({ "a": 2 })),
            tdef: TypeDef::object(Collection::any()),
        }

        longer_keys_array {
            args: func_args![
                keys: value!(["a", "b"]),
                items: value!([1])
            ],
            want: Ok(value!({ "a": 1 })),
            tdef: TypeDef::object(Collection::any()),
        }

        longer_values_array {
            args: func_args![
                keys: value!(["a"]),
                items: value!([1, 2])
            ],
            want: Ok(value!({ "a": 1 })),
            tdef: TypeDef::object(Collection::any()),
        }
    ];
}
