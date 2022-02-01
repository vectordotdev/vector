use std::result::Result;

use vector_common::encode_key_value;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct EncodeKeyValue;

impl Function for EncodeKeyValue {
    fn identifier(&self) -> &'static str {
        "encode_key_value"
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
            Parameter {
                keyword: "key_value_delimiter",
                kind: kind::BYTES,
                required: false,
            },
            Parameter {
                keyword: "field_delimiter",
                kind: kind::BYTES,
                required: false,
            },
            Parameter {
                keyword: "flatten_boolean",
                kind: kind::BOOLEAN,
                required: false,
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
        let fields = arguments.optional("fields_ordering");

        let key_value_delimiter = arguments
            .optional("key_value_delimiter")
            .unwrap_or_else(|| expr!("="));

        let field_delimiter = arguments
            .optional("field_delimiter")
            .unwrap_or_else(|| expr!(" "));

        let flatten_boolean = arguments
            .optional("flatten_boolean")
            .unwrap_or_else(|| expr!(false));

        Ok(Box::new(EncodeKeyValueFn {
            value,
            fields,
            key_value_delimiter,
            field_delimiter,
            flatten_boolean,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "encode object",
                source: r#"encode_key_value({"lvl": "info", "msg": "This is a message", "log_id": 12345})"#,
                result: Ok(r#"s'log_id=12345 lvl=info msg="This is a message"'"#),
            },
            Example {
                title: "encode object with fields ordering",
                source: r#"encode_key_value!({"msg": "This is a message", "lvl": "info", "log_id": 12345}, ["lvl", "msg"])"#,
                result: Ok(r#"s'lvl=info msg="This is a message" log_id=12345'"#),
            },
            Example {
                title: "custom delimiters",
                source: r#"encode_key_value({"start": "ool", "end": "kul", "stop1": "yyc", "stop2" : "gdx"}, key_value_delimiter: ":", field_delimiter: ",")"#,
                result: Ok(r#"s'end:kul,start:ool,stop1:yyc,stop2:gdx'"#),
            },
        ]
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EncodeKeyValueFn {
    pub(crate) value: Box<dyn Expression>,
    pub(crate) fields: Option<Box<dyn Expression>>,
    pub(crate) key_value_delimiter: Box<dyn Expression>,
    pub(crate) field_delimiter: Box<dyn Expression>,
    pub(crate) flatten_boolean: Box<dyn Expression>,
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

impl Expression for EncodeKeyValueFn {
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

        let value = self.key_value_delimiter.resolve(ctx)?;
        let key_value_delimiter = value.try_bytes_utf8_lossy()?;

        let value = self.field_delimiter.resolve(ctx)?;
        let field_delimiter = value.try_bytes_utf8_lossy()?;
        let flatten_boolean = self.flatten_boolean.resolve(ctx)?.try_boolean()?;

        Ok(encode_key_value::to_string(
            object,
            &fields[..],
            &key_value_delimiter,
            &field_delimiter,
            flatten_boolean,
        )
        .expect("Should always succeed.")
        .into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .bytes()
            .with_fallibility(self.fields.is_some())
    }
}

#[cfg(test)]
mod tests {
    use vector_common::btreemap;

    use super::*;

    test_function![
        encode_key_value  => EncodeKeyValue;

        single_element {
            args: func_args![value:
                btreemap! {
                    "lvl" => "info"
                }
            ],
            want: Ok("lvl=info"),
            tdef: TypeDef::new().bytes().infallible(),
        }

        multiple_elements {
            args: func_args![value:
                btreemap! {
                    "lvl" => "info",
                    "log_id" => 12345
                }
            ],
            want: Ok("log_id=12345 lvl=info"),
            tdef: TypeDef::new().bytes().infallible(),
        }

        string_with_spaces {
            args: func_args![value:
                btreemap! {
                    "lvl" => "info",
                    "msg" => "This is a log message"
                }],
            want: Ok(r#"lvl=info msg="This is a log message""#),
            tdef: TypeDef::new().bytes().infallible(),
        }

        flatten_boolean {
            args: func_args![value:
                btreemap! {
                    "beta" => true,
                    "prod" => false,
                    "lvl" => "info",
                    "msg" => "This is a log message",
                },
                flatten_boolean: value!(true)
            ],
            want: Ok(r#"beta lvl=info msg="This is a log message""#),
            tdef: TypeDef::new().bytes().infallible(),
        }

        dont_flatten_boolean {
            args: func_args![value:
                btreemap! {
                    "beta" => true,
                    "prod" => false,
                    "lvl" => "info",
                    "msg" => "This is a log message",
                },
                flatten_boolean: value!(false)
            ],
            want: Ok(r#"beta=true lvl=info msg="This is a log message" prod=false"#),
            tdef: TypeDef::new().bytes().infallible(),
        }

        flatten_boolean_with_custom_delimiters {
            args: func_args![value:
                btreemap! {
                    "tag_a" => "val_a",
                    "tag_b" => "val_b",
                    "tag_c" => true,
                },
                key_value_delimiter: value!(":"),
                field_delimiter: value!(","),
                flatten_boolean: value!(true)
            ],
            want: Ok(r#"tag_a:val_a,tag_b:val_b,tag_c"#),
            tdef: TypeDef::new().bytes().infallible(),
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
            tdef: TypeDef::new().bytes().infallible(),
        }

        nested_fields {
            args: func_args![value:
                btreemap! {
                    "log" => btreemap! {
                        "file" => btreemap! {
                            "path" => "encode_key_value.rs"
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
                want: Ok("agent.id=1234 agent.name=vector event=log log.file.path=encode_key_value.rs network.ip.0=127 network.ip.1=0 network.ip.2=0 network.ip.3=1 network.proto=tcp"),
                tdef: TypeDef::new().bytes().infallible(),
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
                            "path" => "encode_key_value.rs"
                        },
                    },
                    "agent" => btreemap! {
                        "name" => "vector",
                    },
                    "event" => "log"
                },
                fields_ordering:  value!(["event", "log.file.path", "agent.name"])
            ],
            want: Ok("event=log log.file.path=encode_key_value.rs agent.name=vector"),
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
