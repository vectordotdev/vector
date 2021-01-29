use crate::util;
use remap::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct Compact;

impl Function for Compact {
    fn identifier(&self) -> &'static str {
        "compact"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Map(_) | Value::Array(_)),
                required: true,
            },
            Parameter {
                keyword: "recursive",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
            Parameter {
                keyword: "null",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
            Parameter {
                keyword: "string",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
            Parameter {
                keyword: "map",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
            Parameter {
                keyword: "array",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
            Parameter {
                keyword: "nullish",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let recursive = arguments.optional("recursive").map(Expr::boxed);
        let null = arguments.optional("null").map(Expr::boxed);
        let string = arguments.optional("string").map(Expr::boxed);
        let map = arguments.optional("map").map(Expr::boxed);
        let array = arguments.optional("array").map(Expr::boxed);
        let nullish = arguments.optional("nullish").map(Expr::boxed);

        Ok(Box::new(CompactFn {
            value,
            recursive,
            null,
            string,
            map,
            array,
            nullish,
        }))
    }
}

#[derive(Debug, Clone)]
struct CompactFn {
    value: Box<dyn Expression>,
    recursive: Option<Box<dyn Expression>>,
    null: Option<Box<dyn Expression>>,
    string: Option<Box<dyn Expression>>,
    map: Option<Box<dyn Expression>>,
    array: Option<Box<dyn Expression>>,
    nullish: Option<Box<dyn Expression>>,
}

#[derive(Debug)]
struct CompactOptions {
    recursive: bool,
    null: bool,
    string: bool,
    map: bool,
    array: bool,
    nullish: bool,
}

impl Default for CompactOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            null: true,
            string: true,
            map: true,
            array: true,
            nullish: false,
        }
    }
}

impl CompactOptions {
    /// Check if the value is empty according to the given options
    fn is_empty(&self, value: &Value) -> bool {
        if self.nullish && util::is_nullish(&value) {
            return true;
        }

        match value {
            Value::Bytes(bytes) => self.string && bytes.len() == 0,
            Value::Null => self.null,
            Value::Map(map) => self.map && map.is_empty(),
            Value::Array(array) => self.array && array.is_empty(),
            _ => false,
        }
    }
}

impl Expression for CompactFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let options = CompactOptions {
            recursive: match &self.recursive {
                Some(expr) => expr.execute(state, object)?.try_boolean()?,
                None => true,
            },

            null: match &self.null {
                Some(expr) => expr.execute(state, object)?.try_boolean()?,
                None => true,
            },

            string: match &self.string {
                Some(expr) => expr.execute(state, object)?.try_boolean()?,
                None => true,
            },

            map: match &self.map {
                Some(expr) => expr.execute(state, object)?.try_boolean()?,
                None => true,
            },

            array: match &self.array {
                Some(expr) => expr.execute(state, object)?.try_boolean()?,
                None => true,
            },

            nullish: match &self.nullish {
                Some(expr) => expr.execute(state, object)?.try_boolean()?,
                None => false,
            },
        };

        match self.value.execute(state, object)? {
            Value::Map(map) => Ok(Value::from(compact_map(map, &options))),
            Value::Array(arr) => Ok(Value::from(compact_array(arr, &options))),
            _ => unreachable!(),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Map | value::Kind::Array)
    }
}

/// Compact the value if we are recursing - otherwise, just return the value untouched.
fn recurse_compact(value: Value, options: &CompactOptions) -> Value {
    match value {
        Value::Array(array) if options.recursive => Value::from(compact_array(array, options)),
        Value::Map(map) if options.recursive => Value::from(compact_map(map, options)),
        _ => value,
    }
}

fn compact_map(map: BTreeMap<String, Value>, options: &CompactOptions) -> BTreeMap<String, Value> {
    map.into_iter()
        .filter_map(|(key, value)| {
            let value = recurse_compact(value, options);
            if options.is_empty(&value) {
                None
            } else {
                Some((key, value))
            }
        })
        .collect()
}

fn compact_array(array: Vec<Value>, options: &CompactOptions) -> Vec<Value> {
    array
        .into_iter()
        .filter_map(|value| {
            let value = recurse_compact(value, options);
            if options.is_empty(&value) {
                None
            } else {
                Some(value)
            }
        })
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;
    use shared::btreemap;

    #[test]
    fn test_compacted_array() {
        let cases = vec![
            (
                vec!["".into(), "".into()],              // expected
                vec!["".into(), Value::Null, "".into()], // original
                CompactOptions {
                    string: false,
                    ..Default::default()
                },
            ),
            (
                vec![1.into(), 2.into()],
                vec![1.into(), Value::Array(vec![]), 2.into()],
                Default::default(),
            ),
            (
                vec![1.into(), Value::Array(vec![3.into()]), 2.into()],
                vec![
                    1.into(),
                    Value::Array(vec![Value::Null, 3.into(), Value::Null]),
                    2.into(),
                ],
                Default::default(),
            ),
            (
                vec![1.into(), 2.into()],
                vec![
                    1.into(),
                    Value::Array(vec![Value::Null, Value::Null]),
                    2.into(),
                ],
                Default::default(),
            ),
            (
                vec![1.into(), Value::Map(btreemap! { "field2" => 2 }), 2.into()],
                vec![
                    1.into(),
                    Value::Map(btreemap! {
                        "field1" => Value::Null,
                        "field2" => 2,
                    }),
                    2.into(),
                ],
                Default::default(),
            ),
        ];

        for (expected, original, options) in cases {
            assert_eq!(expected, compact_array(original, &options))
        }
    }

    #[test]
    fn test_compacted_map() {
        let cases = vec![
            (
                btreemap! {
                    "key1" => "",
                    "key3" => "",
                }, // expected
                btreemap! {
                    "key1" => "",
                    "key2" => Value::Null,
                    "key3" => "",
                }, // original
                CompactOptions {
                    string: false,
                    ..Default::default()
                },
            ),
            (
                btreemap! {
                    "key1" => Value::from(1),
                    "key3" => Value::from(2),
                },
                btreemap! {
                    "key1" => Value::from(1),
                    "key2" => Value::Array(vec![]),
                    "key3" => Value::from(2),
                },
                Default::default(),
            ),
            (
                btreemap! {
                    "key1" => Value::from(1),
                    "key2" => Value::Map(btreemap! { "key2" => Value::from(3) }),
                    "key3" => Value::from(2),
                },
                btreemap! {
                    "key1" => Value::from(1),
                    "key2" => Value::Map(btreemap! {
                        "key1" => Value::Null,
                        "key2" => Value::from(3),
                        "key3" => Value::Null,
                    }),
                    "key3" => Value::from(2),
                },
                Default::default(),
            ),
            (
                btreemap! {
                    "key1" => Value::from(1),
                    "key2" => Value::Map(btreemap! { "key1" => Value::Null }),
                    "key3" => Value::from(2),
                },
                btreemap! {
                    "key1" => Value::from(1),
                    "key2" => Value::Map(btreemap! { "key1" => Value::Null }),
                    "key3" => Value::from(2),
                },
                CompactOptions {
                    recursive: false,
                    ..Default::default()
                },
            ),
            (
                btreemap! {
                    "key1" => Value::from(1),
                    "key3" => Value::from(2),
                },
                btreemap! {
                    "key1" => Value::from(1),
                    "key2" => Value::Map(btreemap! { "key1" => Value::Null }),
                    "key3" => Value::from(2),
                },
                Default::default(),
            ),
            (
                btreemap! {
                    "key1" => Value::from(1),
                    "key2" => Value::Array(vec![2.into()]),
                    "key3" => Value::from(2),
                },
                btreemap! {
                    "key1" => Value::from(1),
                    "key2" => Value::Array(vec![Value::Null, 2.into(), Value::Null]),
                    "key3" => Value::from(2),
                },
                Default::default(),
            ),
        ];

        for (expected, original, options) in cases {
            assert_eq!(expected, compact_map(original, &options))
        }
    }

    test_function![
        compact => Compact;

        with_map {
            args: func_args![
                value: btreemap! {
                    "key1" => Value::Null,
                    "key2" => 1,
                    "key3" => "",
                }
            ],
            want: Ok(Value::Map(btreemap! { "key2" => 1 })),
        }

        with_array {
            args: func_args![value: vec![Value::Null, Value::from(1), Value::from(""),]],
            want: Ok(Value::Array(vec![Value::from(1)])),
        }

        nullish {
            args: func_args![
                value: btreemap! {
                    "key1" => "-",
                    "key2" => 1,
                    "key3" => " "
                },
                nullish: true
            ],
            want: Ok(Value::Map(btreemap! { "key2" => 1 })),
        }
    ];
}
