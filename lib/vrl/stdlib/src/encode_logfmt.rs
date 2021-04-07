use vrl::prelude::*;

mod logfmt {
    use std::collections::{BTreeMap, HashSet};
    use std::fmt::{self, Write};
    use std::result::Result;

    use vrl::prelude::*;

    fn encode_string(output: &mut String, str: &str) -> fmt::Result {
        let needs_quotting = match str.find(' ') {
            Some(_) => true,
            None => false,
        };

        if needs_quotting {
            output.write_char('"')?;
        }

        for c in str.chars() {
            let needs_escaping = match c {
                '\\' | '"' => true,
                _ => false,
            };

            if needs_escaping {
                output.write_char('\\')?;
            }

            output.write_char(c)?;
        }

        if needs_quotting {
            output.write_char('"')?;
        }

        Ok(())
    }

    fn encode_value(output: &mut String, value: &Value) -> fmt::Result {
        match value {
            Value::Bytes(b) => {
                let val = String::from_utf8_lossy(b);
                encode_string(output, &val)
            }
            _ => {
                let val = format!("{}", value);
                encode_string(output, &val)
            }
        }
    }

    fn encode_field(output: &mut String, key: &str, value: &Value) -> fmt::Result {
        encode_string(output, key)?;
        output.write_char('=')?;
        encode_value(output, value)
    }

    pub fn encode(input: &BTreeMap<String, Value>, fields: &[String]) -> Result<String, String> {
        let mut output = String::new();
        let mut seen_fields = HashSet::new();

        for (idx, field) in fields.iter().enumerate() {
            if let Some(val) = input.get(field) {
                if idx > 0 {
                    output.write_char(' ').map_err(|_| "write error")?;
                }
                encode_field(&mut output, field, val).map_err(|_| "write error")?;
                seen_fields.insert(field);
            }
        }

        for (idx, (key, value)) in input.iter().enumerate() {
            if seen_fields.contains(key) {
                continue;
            }

            if idx > 0 || seen_fields.len() > 0 {
                output.write_char(' ').map_err(|_| "write error")?;
            }
            encode_field(&mut output, key, value).map_err(|_| "write error")?;
        }

        Ok(output)
    }
}

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
                source: r#"encode_logfmt!({"msg": "This is a message", "lod_id": 12345, "lvl": "info"}, ["lvl", "msg"])"#,
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

fn resolve_fields(fields: &Value) -> Vec<String> {
    match fields {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| match v {
                Value::Bytes(bytes) => Some(String::from_utf8_lossy(&bytes)),
                _ => None,
            })
            .map(Into::into)
            .collect(),
        _ => vec![],
    }
}

impl Expression for EncodeLogfmtFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let fields = match &self.fields {
            None => vec![],
            Some(expr) => match expr.resolve(ctx) {
                Ok(val) => resolve_fields(&val),
                Err(_) => vec![],
            },
        };

        let logfmt = match value {
            Value::Object(map) => logfmt::encode(&map, &fields[..]),
            _ => Err("unsupported value-type".into()),
        };

        logfmt
            .map_err(|err| format!("failed to encode logfmt: {}", err).into())
            .map(Into::into)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value.type_def(state).fallible().bytes()
    }
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
                    "msg" => r#"payload: {"code": 200}\n"#
                }],
            want: Ok(r#"lvl=info msg="payload: {\"code\": 200}\\n""#),
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
    ];
}
