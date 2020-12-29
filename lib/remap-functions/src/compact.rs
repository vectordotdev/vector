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
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let recursive = arguments.optional("recursive").map(Expr::boxed);
        let null = arguments.optional("null").map(Expr::boxed);
        let string = arguments.optional("string").map(Expr::boxed);
        let map = arguments.optional("map").map(Expr::boxed);
        let array = arguments.optional("array").map(Expr::boxed);

        Ok(Box::new(CompactFn {
            value,
            recursive,
            null,
            string,
            map,
            array,
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
}

impl CompactFn {
    #[cfg(test)]
    fn new(
        value: Box<dyn Expression>,
        recursive: Option<Box<dyn Expression>>,
        null: Option<Box<dyn Expression>>,
        string: Option<Box<dyn Expression>>,
        map: Option<Box<dyn Expression>>,
        array: Option<Box<dyn Expression>>,
    ) -> Self {
        Self {
            value,
            recursive,
            null,
            string,
            map,
            array,
        }
    }
}

#[derive(Debug)]
struct CompactOptions {
    recursive: bool,
    null: bool,
    string: bool,
    map: bool,
    array: bool,
}

impl Default for CompactOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            null: true,
            string: true,
            map: true,
            array: true,
        }
    }
}

impl CompactOptions {
    /// Check if the value is empty according to the given options
    fn is_empty(&self, value: &Value) -> bool {
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
    use crate::map;

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
                vec![1.into(), Value::Map(map!["field2": 2]), 2.into()],
                vec![
                    1.into(),
                    Value::Map(map!["field1": Value::Null,
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
                map!["key1": "",
                     "key3": ""], // expected
                map!["key1": "",
                     "key2": Value::Null,
                     "key3": ""], // original
                CompactOptions {
                    string: false,
                    ..Default::default()
                },
            ),
            (
                map!["key1": Value::from(1),
                     "key3": Value::from(2)],
                map!["key1": Value::from(1),
                     "key2": Value::Array(vec![]),
                     "key3": Value::from(2)],
                Default::default(),
            ),
            (
                map!["key1": Value::from(1),
                     "key2": Value::Map(map!["key2": Value::from(3)]),
                     "key3": Value::from(2),
                ],
                map![
                    "key1": Value::from(1),
                    "key2": Value::Map(map!["key1": Value::Null,
                                            "key2": Value::from(3),
                                            "key3": Value::Null]),
                    "key3": Value::from(2),
                ],
                Default::default(),
            ),
            (
                map!["key1": Value::from(1),
                     "key2": Value::Map(map!["key1": Value::Null,]),
                     "key3": Value::from(2),
                ],
                map![
                    "key1": Value::from(1),
                    "key2": Value::Map(map!["key1": Value::Null,]),
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
                    "key2": Value::Map(map!["key1": Value::Null,]),
                    "key3": Value::from(2),
                ],
                Default::default(),
            ),
            (
                map!["key1": Value::from(1),
                     "key2": Value::Array(vec![2.into()]),
                     "key3": Value::from(2),
                ],
                map![
                    "key1": Value::from(1),
                    "key2": Value::Array(vec![Value::Null, 2.into(), Value::Null]),
                    "key3": Value::from(2),
                ],
                Default::default(),
            ),
        ];

        for (expected, original, options) in cases {
            assert_eq!(expected, compact_map(original, &options))
        }
    }

    #[test]
    fn compact() {
        let cases = vec![
            (
                map![
                    "foo":
                        map!["key1": Value::Null,
                             "key2": 1,
                             "key3": "",
                        ]
                ],
                Ok(Value::Map(map!["key2": 1])),
                CompactFn::new(
                    Box::new(Path::from("foo")),
                    Some(Literal::from(true).boxed()),
                    Some(Literal::from(true).boxed()),
                    Some(Literal::from(true).boxed()),
                    Some(Literal::from(true).boxed()),
                    Some(Literal::from(true).boxed()),
                ),
            ),
            (
                map!["foo": vec![Value::Null, Value::from(1), Value::from(""),]],
                Ok(Value::Array(vec![Value::from(1)])),
                CompactFn::new(
                    Box::new(Path::from("foo")),
                    Some(Literal::from(true).boxed()),
                    Some(Literal::from(true).boxed()),
                    Some(Literal::from(true).boxed()),
                    Some(Literal::from(true).boxed()),
                    Some(Literal::from(true).boxed()),
                ),
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
