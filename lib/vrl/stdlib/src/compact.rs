use crate::util;
use std::collections::BTreeMap;
use vrl::prelude::*;

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
                kind: kind::OBJECT | kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "recursive",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "null",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "string",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "map",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "array",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "nullish",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "object",
                source: r#"compact({ "a": {}, "b": null, "c": [null], "d": "", "e": "-", "f": true })"#,
                result: Ok(r#"{ "e": "-", "f": true }"#),
            },
            Example {
                title: "nullish",
                source: r#"compact(["-", "   ", "\n", null, true], nullish: true)"#,
                result: Ok(r#"[true]"#),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let recursive = arguments.optional("recursive");
        let null = arguments.optional("null");
        let string = arguments.optional("string");
        let map = arguments.optional("map");
        let array = arguments.optional("array");
        let nullish = arguments.optional("nullish");

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
            Value::Object(map) => self.map && map.is_empty(),
            Value::Array(array) => self.array && array.is_empty(),
            _ => false,
        }
    }
}

impl Expression for CompactFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let options = CompactOptions {
            recursive: match &self.recursive {
                Some(expr) => expr.resolve(ctx)?.unwrap_boolean(),
                None => true,
            },

            null: match &self.null {
                Some(expr) => expr.resolve(ctx)?.unwrap_boolean(),
                None => true,
            },

            string: match &self.string {
                Some(expr) => expr.resolve(ctx)?.unwrap_boolean(),
                None => true,
            },

            map: match &self.map {
                Some(expr) => expr.resolve(ctx)?.unwrap_boolean(),
                None => true,
            },

            array: match &self.array {
                Some(expr) => expr.resolve(ctx)?.unwrap_boolean(),
                None => true,
            },

            nullish: match &self.nullish {
                Some(expr) => expr.resolve(ctx)?.unwrap_boolean(),
                None => false,
            },
        };

        match self.value.resolve(ctx)? {
            Value::Object(map) => Ok(Value::from(compact_map(map, &options))),
            Value::Array(arr) => Ok(Value::from(compact_array(arr, &options))),
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

/// Compact the value if we are recursing - otherwise, just return the value untouched.
fn recurse_compact(value: Value, options: &CompactOptions) -> Value {
    match value {
        Value::Array(array) if options.recursive => Value::from(compact_array(array, options)),
        Value::Object(map) if options.recursive => Value::from(compact_map(map, options)),
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

/*
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
                vec![1.into(), Value::Object(map!["field2": 2]), 2.into()],
                vec![
                    1.into(),
                    Value::Object(map!["field1": Value::Null,
                                    "field2": 2]),
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
                map!["key1": Value::from(1),
                     "key2": Value::Object(map!["key2": Value::from(3)]),
                     "key3": Value::from(2),
                ],
                map![
                    "key1": Value::from(1),
                    "key2": Value::Object(map!["key1": Value::Null,
                                            "key2": Value::from(3),
                                            "key3": Value::Null]),
                    "key3": Value::from(2),
                ],
                Default::default(),
            ),
            (
                map!["key1": Value::from(1),
                     "key2": Value::Object(map!["key1": Value::Null,]),
                     "key3": Value::from(2),
                ],
                map![
                    "key1": Value::from(1),
                    "key2": Value::Object(map!["key1": Value::Null,]),
                    "key3": Value::from(2),
                ],
                CompactOptions {
                    recursive: false,
                    ..Default::default()
                },
            ),
            (
                map!["key1": Value::from(1),
                     "key3": Value::from(2),
                ],
                map![
                    "key1": Value::from(1),
                    "key2": Value::Object(map!["key1": Value::Null,]),
                    "key3": Value::from(2),
                ],
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
            args: func_args![value: map!["key1": Value::Null,
                                         "key2": 1,
                                         "key3": "",
            ]],
            want: Ok(Value::Object(map!["key2": 1])),
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
            want: Ok(Value::Object(map!["key2": 1])),
        }
    ];
}
*/
