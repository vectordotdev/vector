use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct EncodeLogfmt;

impl Function for EncodeLogfmt {
    fn identifier(&self) -> &'static str {
        "encode_logfmt"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::OBJECT | kind::ARRAY,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(EncodeLogfmtFn { value }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "encode object",
                source: r#"encode_logfmt({"lvl": "info", "msg": "This is a message", "log_id": 12345})"#,
                result: Ok(r#"s'log_id=12345 lvl=info msg="This is a message"'"#),
            },
            Example {
                title: "encode array",
                source: r#"encode_logfmt([["lvl", "info"], ["msg", "This is a message"], ["log_id", 12345]])"#,
                result: Ok(r#"s'lvl=info msg="This is a message" log_id=12345'"#),
            },
        ]
    }
}

fn format_logfmt_string(f: &mut fmt::Formatter<'_>, str: &str) -> fmt::Result {
    match str.find(' ') {
        Some(_) => write!(f, r#""{}""#, str),
        None => write!(f, "{}", str),
    }
}

struct LogFmtFieldValueFormatter<'a> {
    value: &'a Value,
}

impl std::fmt::Display for LogFmtFieldValueFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value {
            Value::Bytes(b) => {
                let val = String::from_utf8_lossy(b);
                format_logfmt_string(f, &val)
            }
            _ => {
                let val = format!("{}", self.value);
                format_logfmt_string(f, &val)
            }
        }
    }
}

struct LogFmtFieldFormatter<'a> {
    key: &'a str,
    value: &'a Value,
}

impl std::fmt::Display for LogFmtFieldFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_logfmt_string(f, self.key)?;
        let value_format = LogFmtFieldValueFormatter { value: self.value };
        write!(f, "={}", value_format)
    }
}

struct LogFmtFormatter {
    value: Value,
}

impl std::fmt::Display for LogFmtFormatter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value {
            Value::Object(map) => {
                for (idx, (key, value)) in map.iter().enumerate() {
                    let field_formatter = LogFmtFieldFormatter { key, value };
                    if idx > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{}", field_formatter)?;
                }

                Ok(())
            }
            Value::Array(arr) => {
                for (idx, value) in arr.iter().enumerate() {
                    if idx > 0 {
                        f.write_str(" ")?;
                    }

                    match value {
                        Value::Array(arr) if arr.len() == 2 => {
                            let (key, value) = (&arr[0], &arr[1]);
                            if let Value::Bytes(b) = key {
                                let key_str = String::from_utf8_lossy(b);
                                let field_formatter = LogFmtFieldFormatter {
                                    key: &key_str,
                                    value,
                                };
                                write!(f, "{}", field_formatter)?;
                            }
                        }
                        _ => return Err(fmt::Error),
                    }
                }

                Ok(())
            }
            _ => Err(fmt::Error),
        }
    }
}

#[derive(Clone, Debug)]
struct EncodeLogfmtFn {
    value: Box<dyn Expression>,
}

impl Expression for EncodeLogfmtFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let formatter = LogFmtFormatter { value };

        let mut output = String::new();
        match std::fmt::write(&mut output, format_args!("{}", formatter)) {
            Ok(_) => Ok(output.into()),
            Err(_) => Err("Failed to encode logfmt".into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value.type_def(state).infallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        encode_logfmt  => EncodeLogfmt;

        array_with_single_element_type {
            args: func_args![value: value!(
                      vec![value!(vec!["lvl", "info"])]
                      )
                    ],
            want: Ok("lvl=info"),
            tdef: TypeDef::new().bytes().infallible(),
        }

        array_with_multiple_element_types {
            args: func_args![value: value!(
                      vec![
                          value!(vec![value!("lvl"), value!("info")]),
                          value!(vec![value!("log_id"), value!(12345)]),
                      ]
                  )],
            want: Ok("lvl=info log_id=12345"),
            tdef: TypeDef::new().bytes().infallible(),
        }

        array_with_too_many_items_in_sub_array_error {
            args: func_args![value: value!(
                      vec![
                          value!(vec![value!("lvl"), value!("info"), value!("error")]),
                          value!(vec![value!("log_id"), value!(12345)]),
                      ]
                  )],
            want: Err("Failed to encode logfmt"),
            tdef: TypeDef::new().bytes().infallible(),
        }

        array_with_missing_items_in_sub_array_error {
            args: func_args![value: value!(
                      vec![
                          value!(vec![value!("log_id"), value!(12345)]),
                          value!(vec![value!("lvl")]),
                      ]
                  )],
            want: Err("Failed to encode logfmt"),
            tdef: TypeDef::new().bytes().infallible(),
        }

        array_with_string_and_spaces {
            args: func_args![value: value!(
                      vec![
                          value!(vec![value!("lvl"), value!("info")]),
                          value!(vec![value!("msg"), value!("This is a log message")]),
                      ]
                  )],
            want: Ok(r#"lvl=info msg="This is a log message""#),
            tdef: TypeDef::new().bytes().infallible(),
        }

        map_with_single_element_type {
            args: func_args![value: value!(map!["lvl": value!("info")])],
            want: Ok("lvl=info"),
            tdef: TypeDef::new().bytes().infallible(),
        }

        map_with_multiple_element_types {
            args: func_args![value: value!(map!["lvl": value!("info"), "log_id": value!(12345)])],
            want: Ok("log_id=12345 lvl=info"),
            tdef: TypeDef::new().bytes().infallible(),
        }

        map_with_string_and_spaces {
            args: func_args![value: value!(map!["lvl": value!("info"), "msg": value!("This is a log message")])],
            want: Ok(r#"lvl=info msg="This is a log message""#),
            tdef: TypeDef::new().bytes().infallible(),
        }
    ];
}
