use shared::btreemap;
use std::collections::BTreeMap;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct TagTypesExternally;

impl Function for TagTypesExternally {
    fn identifier(&self) -> &'static str {
        "tag_types_externally"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "object",
            source: indoc! {r#"
                tag_types_externally({
                    "message": "Hello world",
                    "request": {
                        "duration_ms": 67.9
                    }
                })
            "#},
            result: Ok(
                r#"{ "message": { "bytes": "Hello world" }, "request": { "duration_ms": { "float": 67.9 } } }"#,
            ),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(TagTypesExternallyFn { value }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }
}

#[derive(Debug, Clone)]
struct TagTypesExternallyFn {
    value: Box<dyn Expression>,
}

impl Expression for TagTypesExternallyFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let tagged_externally = tag_type_externally(value);

        Ok(tagged_externally)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .infallible()
            .object::<(), Kind>(map! {
                (): Kind::all()
            })
            .add_array_mapped::<(), Kind>(map! {
                (): Kind::all()
            })
    }
}

fn tag_type_externally(value: Value) -> Value {
    let (key, value) = match value {
        value @ Value::Bytes(_) => (Some("bytes"), value),
        value @ Value::Integer(_) => (Some("integer"), value),
        value @ Value::Float(_) => (Some("float"), value),
        value @ Value::Boolean(_) => (Some("boolean"), value),
        Value::Object(object) => (
            None,
            object
                .into_iter()
                .map(|(key, value)| (key, tag_type_externally(value)))
                .collect::<BTreeMap<String, Value>>()
                .into(),
        ),
        Value::Array(array) => (
            None,
            array
                .into_iter()
                .map(tag_type_externally)
                .collect::<Vec<_>>()
                .into(),
        ),
        value @ Value::Timestamp(_) => (Some("timestamp"), value),
        value @ Value::Regex(_) => (Some("regex"), value),
        Value::Null => (Some("null"), Value::Null),
    };

    if let Some(key) = key {
        (btreemap! {
            key => value
        })
        .into()
    } else {
        value
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use regex::Regex;
    use shared::btreemap;

    test_function![
        tag_types_externally => TagTypesExternally;

        tag_bytes {
            args: btreemap! {
                "value" => "foo"
            },
            want: Ok(btreemap! {
                "bytes" => "foo",
            }),
        }

        tag_integer {
            args: btreemap! {
                "value" => 123
            },
            want: Ok(btreemap! {
                "integer" => 123
            })
        }

        tag_float {
            args: btreemap! {
                "value" => 123.45
            },
            want: Ok(btreemap! {
                "float" => 123.45
            })
        }

        tag_boolean {
            args: btreemap! {
                "value" => true
            },
            want: Ok(btreemap! {
                "boolean" => true
            })
        }

        tag_map {
            args: btreemap! {
                "value" => btreemap! {
                    "foo" => "bar"
                }
            },
            want: Ok(btreemap! {
                "foo" => btreemap! {
                    "bytes" => "bar"
                }
            })
        }

        tag_array {
            args: btreemap! {
                "value" => vec!["foo"]
            },
            want: Ok(vec![
                btreemap! {
                    "bytes" => "foo"
                },
            ])
        }

        tag_timestamp {
            args: btreemap! {
                "value" => Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0)
            },
            want: Ok(btreemap! {
                "timestamp" => Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0)
            })
        }

        tag_regex {
            args: btreemap! {
                "value" => Regex::new(".*").unwrap()
            },
            want: Ok(btreemap! {
                "regex" => Regex::new(".*").unwrap()
            })
        }

        tag_null {
            args: btreemap! {
                "value" => Value::Null
            },
            want: Ok(btreemap! {
                "null" => Value::Null
            })
        }
    ];

    test_type_def![
        value_bytes {
            expr: |_| TagTypesExternallyFn { value: lit!("foo").boxed() },
            def: TypeDef { fallible: false, kind: value::Kind::Map | value::Kind::Array, ..Default::default() },
        }

        value_integer {
            expr: |_| TagTypesExternallyFn { value: lit!(123).boxed() },
            def: TypeDef { fallible: false, kind: value::Kind::Map | value::Kind::Array, ..Default::default() },
        }

        value_float {
            expr: |_| TagTypesExternallyFn { value: lit!(123.45).boxed() },
            def: TypeDef { fallible: false, kind: value::Kind::Map | value::Kind::Array, ..Default::default() },
        }

        value_boolean {
            expr: |_| TagTypesExternallyFn { value: lit!(true).boxed() },
            def: TypeDef { fallible: false, kind: value::Kind::Map | value::Kind::Array, ..Default::default() },
        }

        value_map {
            expr: |_| TagTypesExternallyFn { value: map!{}.boxed() },
            def: TypeDef { fallible: false, kind: value::Kind::Map | value::Kind::Array, ..Default::default() },
        }

        value_array {
            expr: |_| TagTypesExternallyFn { value: array![].boxed() },
            def: TypeDef { fallible: false, kind: value::Kind::Map | value::Kind::Array, ..Default::default() },
        }

        value_timestamp {
            expr: |_| TagTypesExternallyFn { value: lit!(Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0)).boxed() },
            def: TypeDef { fallible: false, kind: value::Kind::Map | value::Kind::Array, ..Default::default() },
        }

        value_regex {
            expr: |_| TagTypesExternallyFn { value: lit!(Regex::new(".*").unwrap()).boxed() },
            def: TypeDef { fallible: false, kind: value::Kind::Map | value::Kind::Array, ..Default::default() },
        }

        value_null {
            expr: |_| TagTypesExternallyFn { value: lit!(Value::Null).boxed() },
            def: TypeDef { fallible: false, kind: value::Kind::Map | value::Kind::Array, ..Default::default() },
        }
    ];
}
*/
