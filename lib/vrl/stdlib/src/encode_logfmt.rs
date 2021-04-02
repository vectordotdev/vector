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

mod logfmt {
    use std::collections::BTreeMap;
    use std::fmt::{self, Write};
    use std::result::Result;

    use vrl::prelude::*;

    fn encode_string(output: &mut String, str: &str) -> fmt::Result {
        let needs_quotting = match str.find(' ') {
            Some(_) => true,
            None => false
        };

        if needs_quotting {
            output.write_char('"')?;
        }

        for c in str.chars() {
            let needs_escaping = match c {
                '\\' | '"' => true,
                _ => false
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

    pub fn encode_object(input: &BTreeMap<String, Value>) -> Result<String, String> {
        let mut output = String::new();

        for (idx, (key, value)) in input.iter().enumerate() {
            if idx > 0 {
                output.write_char(' ').map_err(|_| "write error")?;
            }

            encode_field(&mut output, key, value).map_err(|_| "write error")?;
        }

        Ok(output)
    }

    pub fn encode_array(input: &Vec<Value>) -> std::result::Result<String, String> {
        let mut output = String::new();

        for (idx, value) in input.iter().enumerate() {
            if idx > 0 {
                output.write_char(' ').map_err(|_| "write error")?;
            }

            match value {
                Value::Array(arr) if arr.len() == 2 => {
                    let (key, value) = (&arr[0], &arr[1]);
                    if let Value::Bytes(b) = key {
                        let key_str = String::from_utf8_lossy(b);
                        encode_field(&mut output, &key_str, value).map_err(|_| "write error")?;
                    } else {
                        return Err(format!("invalid key type at index {}", idx))
                    }

                }
                _ => return Err(format!("invalid key-value pair at index {}", idx))
            }
        }

        Ok(output)
    }
}


#[derive(Clone, Debug)]
struct EncodeLogfmtFn {
    value: Box<dyn Expression>,
}

impl Expression for EncodeLogfmtFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        let logfmt = match value {
            Value::Object(map) => logfmt::encode_object(&map),
            Value::Array(arr) => logfmt::encode_array(&arr),
            _ => Err("unsupported value-type".into())
        };

        logfmt
            .map_err(|err| format!("failed to encode logfmt: {}", err).into())
            .map(Into::into)
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
            want: Err("failed to encode logfmt: invalid key-value pair at index 0"),
            tdef: TypeDef::new().bytes().infallible(),
        }

        array_with_missing_items_in_sub_array_error {
            args: func_args![value: value!(
                      vec![
                          value!(vec![value!("log_id"), value!(12345)]),
                          value!(vec![value!("lvl")]),
                      ]
                  )],
            want: Err("failed to encode logfmt: invalid key-value pair at index 1"),
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

        array_with_string_and_characters_to_escape {
            args: func_args![value: value!(
                      vec![
                          value!(vec![value!("lvl"), value!("info")]),
                          value!(vec![value!("msg"), value!(r#"payload: {"code": 200}\n"#)]),
                      ]
                  )],
            want: Ok(r#"lvl=info msg="payload: {\"code\": 200}\\n""#),
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

        map_with_string_and_characters_to_escape {
            args: func_args![value: value!(map!["lvl": value!("info"), "msg": value!(r#"payload: {"code": 200}\n"#)])],
            want: Ok(r#"lvl=info msg="payload: {\"code\": 200}\\n""#),
            tdef: TypeDef::new().bytes().infallible(),
        }
    ];
}
