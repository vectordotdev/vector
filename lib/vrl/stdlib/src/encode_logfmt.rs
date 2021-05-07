use std::collections::BTreeMap;
use std::fmt::Write;
use std::result::Result;
use vrl::prelude::*;
use Value::{Array, Object};

#[derive(Clone, Copy, Debug)]
pub struct EncodeLogfmt;

impl Function for EncodeLogfmt {
    fn identifier(&self) -> &'static str {
        "encode_logfmt"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::OBJECT,
                required: true,
            },
            Parameter {
                keyword: "fields_ordering",
                kind: kind::ARRAY,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let fields = arguments.optional("fields_ordering");

        Ok(Box::new(EncodeLogfmtFn { value, fields }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "encode object",
                source: r#"encode_logfmt!({"lvl": "info", "msg": "This is a message", "log_id": 12345})"#,
                result: Ok(r#"s'log_id=12345 lvl=info msg="This is a message"'"#),
            },
            Example {
                title: "encode object with fields ordering",
                source: r#"encode_logfmt!({"msg": "This is a message", "lvl": "info", "log_id": 12345}, ["lvl", "msg"])"#,
                result: Ok(r#"s'lvl=info msg="This is a message" log_id=12345'"#),
            },
        ]
    }
}

#[derive(Clone, Debug)]
struct EncodeLogfmtFn {
    value: Box<dyn Expression>,
    fields: Option<Box<dyn Expression>>,
}

fn resolve_fields(fields: Value) -> Result<Vec<String>, ExpressionError> {
    let arr = fields.try_array()?;
    arr.iter()
        .enumerate()
        .map(|(idx, v)| {
            v.try_bytes_utf8_lossy()
                .map(|v| v.to_string())
                .map_err(|e| format!("invalid field value type at index {}: {}", idx, e).into())
        })
        .collect()
}

impl Expression for EncodeLogfmtFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let fields = match &self.fields {
            None => Ok(vec![]),
            Some(expr) => {
                let fields = expr.resolve(ctx)?;
                resolve_fields(fields)
            }
        }?;

        let object = value.try_object()?;
        Ok(encode(object, &fields[..]).into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().bytes().fallible()
    }
}

fn encode_string(output: &mut String, str: &str) {
    let needs_quoting = str.chars().any(char::is_whitespace);

    if needs_quoting {
        output.write_char('"').unwrap();
    }

    for c in str.chars() {
        match c {
            '\\' => output.write_str(r#"\\"#).unwrap(),
            '"' => output.write_str(r#"\""#).unwrap(),
            '\n' => output.write_str(r#"\\n"#).unwrap(),
            _ => output.write_char(c).unwrap(),
        }
    }

    if needs_quoting {
        output.write_char('"').unwrap();
    }
}

fn encode_value(output: &mut String, value: &Value) {
    match value {
        Value::Bytes(b) => {
            let val = String::from_utf8_lossy(b);
            encode_string(output, &val)
        }
        _ => encode_string(output, &value.to_string()),
    }
}

fn flatten<'a>(
    input: impl IntoIterator<Item = (String, Value)> + 'a,
    parent_key: String,
    separator: char,
    depth: usize,
) -> Box<dyn Iterator<Item = (String, Value)> + 'a> {
    let iter = input.into_iter().map(move |(key, value)| {
        let new_key = if depth > 0 {
            format!("{}{}{}", parent_key, separator, key)
        } else {
            key
        };

        match value {
            Object(map) => flatten(map, new_key, separator, depth + 1),
            Array(array) => {
                let array_map: BTreeMap<_, _> = array
                    .into_iter()
                    .enumerate()
                    .map(|(key, value)| (key.to_string(), value))
                    .collect();
                flatten(array_map, new_key, separator, depth + 1)
            }
            _ => Box::new(std::iter::once((new_key, value)))
                as Box<dyn Iterator<Item = (std::string::String, vrl::Value)>>,
        }
    });

    Box::new(iter.flatten())
}

fn encode_field(output: &mut String, key: &str, value: &Value) {
    encode_string(output, key);
    output.write_char('=').unwrap();
    encode_value(output, value)
}

pub fn encode(input: BTreeMap<String, Value>, fields: &[String]) -> String {
    let mut output = String::new();

    let mut input: BTreeMap<_, _> = flatten(input, String::from(""), '.', 0).collect();

    for field in fields.iter() {
        if let Some(val) = input.remove(field) {
            encode_field(&mut output, field, &val);
            output.write_char(' ').unwrap();
        }
    }

    for (key, value) in input.iter() {
        encode_field(&mut output, key, value);
        output.write_char(' ').unwrap();
    }

    if output.ends_with(' ') {
        output.truncate(output.len() - 1)
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    test_function![
        encode_logfmt  => EncodeLogfmt;

        single_element {
            args: func_args![value:
                btreemap! {
                    "lvl" => "info"
                }
            ],
            want: Ok("lvl=info"),
            tdef: TypeDef::new().bytes().fallible(),
        }

        multiple_elements {
            args: func_args![value:
                btreemap! {
                    "lvl" => "info",
                    "log_id" => 12345
                }
            ],
            want: Ok("log_id=12345 lvl=info"),
            tdef: TypeDef::new().bytes().fallible(),
        }

        string_with_spaces {
            args: func_args![value:
                btreemap! {
                    "lvl" => "info",
                    "msg" => "This is a log message"
                }],
            want: Ok(r#"lvl=info msg="This is a log message""#),
            tdef: TypeDef::new().bytes().fallible(),
        }

        string_with_characters_to_escape {
            args: func_args![value:
                btreemap! {
                    "lvl" => "info",
                    "msg" => r#"payload: {"code": 200}\n"#,
                    "another_field" => "some\nfield\\and things",
                    "space key" => "foo"
                }],
            want: Ok(r#"another_field="some\\nfield\\and things" lvl=info msg="payload: {\"code\": 200}\\n" "space key"=foo"#),
            tdef: TypeDef::new().bytes().fallible(),
        }

        nested_fields {
            args: func_args![value:
                btreemap! {
                    "log" => btreemap! {
                        "file" => btreemap! {
                            "path" => "encode_logfmt.rs"
                        },
                    },
                    "agent" => btreemap! {
                        "name" => "vector",
                        "id" => 1234
                    },
                    "network" => btreemap! {
                        "ip" => value!([127, 0, 0, 1]),
                        "proto" => "tcp"
                    },
                    "event" => "log"
                }],
                want: Ok("agent.id=1234 agent.name=vector event=log log.file.path=encode_logfmt.rs network.ip.0=127 network.ip.1=0 network.ip.2=0 network.ip.3=1 network.proto=tcp"),
                tdef: TypeDef::new().bytes().fallible(),
        }

        fields_ordering {
            args: func_args![value:
                btreemap! {
                    "lvl" => "info",
                    "msg" => "This is a log message",
                    "log_id" => 12345,
                },
                fields_ordering: value!(["lvl", "msg"])
            ],
            want: Ok(r#"lvl=info msg="This is a log message" log_id=12345"#),
            tdef: TypeDef::new().bytes().fallible(),
        }

        nested_fields_ordering {
            args: func_args![value:
                btreemap! {
                    "log" => btreemap! {
                        "file" => btreemap! {
                            "path" => "encode_logfmt.rs"
                        },
                    },
                    "agent" => btreemap! {
                        "name" => "vector",
                    },
                    "event" => "log"
                },
                fields_ordering:  value!(["event", "log.file.path", "agent.name"])
            ],
            want: Ok("event=log log.file.path=encode_logfmt.rs agent.name=vector"),
            tdef: TypeDef::new().bytes().fallible(),
        }

        fields_ordering_invalid_field_type {
            args: func_args![value:
                btreemap! {
                    "lvl" => "info",
                    "msg" => "This is a log message",
                    "log_id" => 12345,
                },
                fields_ordering: value!(["lvl", 2])
            ],
            want: Err(format!(r"invalid field value type at index 1: {}",
                    value::Error::Expected {
                        got: Kind::Integer,
                        expected: Kind::Bytes
                    })),
            tdef: TypeDef::new().bytes().fallible(),
        }
    ];
}
