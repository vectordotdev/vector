use std::collections::BTreeMap;
use std::fmt::Write;
use vrl_compiler::Value;
use Value::{Array, Boolean, Object};

pub fn encode<'a>(
    input: impl IntoIterator<Item = (String, Value)> + 'a,
    fields: &[String],
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
    flatten_boolean: bool,
) -> String {
    let mut output = String::new();

    let mut input: BTreeMap<_, _> = flatten(input, String::from(""), '.', 0).collect();

    for field in fields.iter() {
        match (input.remove(field), flatten_boolean) {
            (Some(Boolean(false)), true) => (),
            (Some(Boolean(true)), true) => {
                encode_string(&mut output, field);
                output.write_str(field_delimiter).unwrap();
            }
            (Some(val), _) => {
                encode_field(&mut output, field, &val, key_value_delimiter);
                output.write_str(field_delimiter).unwrap();
            }
            (None, _) => (),
        };
    }

    for (key, value) in input.iter() {
        match (value, flatten_boolean) {
            (Boolean(false), true) => (),
            (Boolean(true), true) => {
                encode_string(&mut output, key);
                output.write_str(field_delimiter).unwrap();
            }
            (_, _) => {
                encode_field(&mut output, key, value, key_value_delimiter);
                output.write_str(field_delimiter).unwrap();
            }
        };
    }

    if output.ends_with(field_delimiter) {
        output.truncate(output.len() - field_delimiter.len())
    }

    output
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
                as Box<dyn Iterator<Item = (String, Value)>>,
        }
    });

    Box::new(iter.flatten())
}

fn encode_field<'a>(output: &mut String, key: &str, value: &Value, key_value_delimiter: &'a str) {
    encode_string(output, key);
    output.write_str(key_value_delimiter).unwrap();
    encode_value(output, value)
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

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use vrl_compiler::value;

    #[test]
    fn single_element() {
        assert_eq!(
            &encode(
                btreemap! {
                    "lvl" => "info"
                },
                &[],
                "=",
                " ",
                true
            ),
            "lvl=info"
        )
    }

    #[test]
    fn multiple_elements() {
        assert_eq!(
            &encode(
                btreemap! {
                    "lvl" => "info",
                    "log_id" => 12345
                },
                &[],
                "=",
                " ",
                true
            ),
            "log_id=12345 lvl=info"
        )
    }

    #[test]
    fn string_with_spaces() {
        assert_eq!(
            &encode(
                btreemap! {
                    "lvl" => "info",
                    "msg" => "This is a log message"
                },
                &[],
                "=",
                " ",
                true
            ),
            r#"lvl=info msg="This is a log message""#
        )
    }

    #[test]
    fn flatten_boolean() {
        assert_eq!(
            &encode(
                btreemap! {
                    "beta" => true,
                    "prod" => false,
                    "lvl" => "info",
                    "msg" => "This is a log message",
                },
                &[],
                "=",
                " ",
                true
            ),
            r#"beta lvl=info msg="This is a log message""#
        )
    }

    #[test]
    fn dont_flatten_boolean() {
        assert_eq!(
            &encode(
                btreemap! {
                    "beta" => true,
                    "prod" => false,
                    "lvl" => "info",
                    "msg" => "This is a log message",
                },
                &[],
                "=",
                " ",
                false
            ),
            r#"beta=true lvl=info msg="This is a log message" prod=false"#
        )
    }

    #[test]
    fn other_delimiters() {
        assert_eq!(
            &encode(
                btreemap! {
                    "tag_a" => "val_a",
                    "tag_b" => "val_b",
                    "tag_c" => true,
                },
                &[],
                ":",
                ",",
                true
            ),
            r#"tag_a:val_a,tag_b:val_b,tag_c"#
        )
    }

    #[test]
    fn string_with_characters_to_escape() {
        assert_eq!(
            &encode(
                btreemap! {
                    "lvl" => "info",
                    "msg" => r#"payload: {"code": 200}\n"#,
                    "another_field" => "some\nfield\\and things",
                    "space key" => "foo"
                },
                &[],
                "=",
                " ",
                true
            ),
            r#"another_field="some\\nfield\\and things" lvl=info msg="payload: {\"code\": 200}\\n" "space key"=foo"#
        )
    }

    #[test]
    fn nested_fields() {
        assert_eq!(
                &encode(
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
                    },
                    &[],
                    "=",
                    " ",
                    true
                )
                ,
                "agent.id=1234 agent.name=vector event=log log.file.path=encode_key_value.rs network.ip.0=127 network.ip.1=0 network.ip.2=0 network.ip.3=1 network.proto=tcp"
            )
    }

    #[test]
    fn fields_ordering() {
        assert_eq!(
            &encode(
                btreemap! {
                    "lvl" => "info",
                    "msg" => "This is a log message",
                    "log_id" => 12345,
                },
                &["lvl".to_string(), "msg".to_string()],
                "=",
                " ",
                true
            ),
            r#"lvl=info msg="This is a log message" log_id=12345"#
        )
    }

    #[test]
    fn nested_fields_ordering() {
        assert_eq!(
            &encode(
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
                &[
                    "event".to_owned(),
                    "log.file.path".to_owned(),
                    "agent.name".to_owned()
                ],
                "=",
                " ",
                true
            ),
            "event=log log.file.path=encode_key_value.rs agent.name=vector"
        )
    }
}
