use regex::Regex;
use remap::prelude::*;

use crate::util;

#[derive(Clone, Copy, Debug)]
pub struct ParseRegex;

impl Function for ParseRegex {
    fn identifier(&self) -> &'static str {
        "parse_regex"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |_| true,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let pattern = arguments.required_regex("pattern")?;

        Ok(Box::new(ParseRegexFn { value, pattern }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParseRegexFn {
    value: Box<dyn Expression>,
    pattern: Regex,
}

impl Expression for ParseRegexFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        let parsed = self
            .pattern
            .captures(&value)
            .map(|capture| util::capture_regex_to_map(&self.pattern, capture))
            .ok_or("unable to parse regular expression")?;

        Ok(parsed.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .into_fallible(true)
            .with_inner_type(Some(util::regex_type_def(&self.pattern)))
            .with_constraint(value::Kind::Map)
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use super::*;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| ParseRegexFn {
                value: Literal::from("foo").boxed(),
                pattern: Regex::new("^(?P<group>.*)$").unwrap(),
            },
            def: TypeDef { kind: Kind::Map,
                           fallible: true,
                           inner_type_def: Some(inner_type_def! ({ "0": Kind::Bytes,
                                                                   "1": Kind::Bytes,
                                                                   "group": Kind::Bytes
                           })) },
        }

        value_non_string {
            expr: |_| ParseRegexFn {
                value: Literal::from(1).boxed(),
                pattern: Regex::new("^(?P<group>.*)$").unwrap(),
            },
            def: TypeDef { fallible: true,
                           kind: Kind::Map,
                           inner_type_def: Some(inner_type_def! ({ "0": Kind::Bytes,
                                                                   "1": Kind::Bytes,
                                                                   "group": Kind::Bytes
                           })),
            },
        }

        value_optional {
            expr: |_| ParseRegexFn {
                value: Box::new(Noop),
                pattern: Regex::new("^(?P<group>.*)$").unwrap(),
            },
            def: TypeDef { fallible: true,
                           kind: Kind::Map,
                           inner_type_def: Some(inner_type_def! ({ "0": Kind::Bytes,
                                                                   "1": Kind::Bytes,
                                                                   "group": Kind::Bytes
                           }))
            },
        }
    ];

    test_function![
        find => ParseRegex;

        matches {
            args: func_args! [
                value: "5.86.210.12 - zieme4647 5667 [19/06/2019:17:20:49 -0400] \"GET /embrace/supply-chains/dynamic/vertical\" 201 20574",
                pattern: Regex::new(r#"^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$"#)
                    .unwrap()
            ],
            want: Ok(value!({"bytes_in": "5667",
                             "host": "5.86.210.12",
                             "user": "zieme4647",
                             "timestamp": "19/06/2019:17:20:49 -0400",
                             "method": "GET",
                             "path": "/embrace/supply-chains/dynamic/vertical",
                             "status": "201",
                             "bytes_out": "20574",
                             "0": "5.86.210.12 - zieme4647 5667 [19/06/2019:17:20:49 -0400] \"GET /embrace/supply-chains/dynamic/vertical\" 201 20574",
                             "1": "5.86.210.12",
                             "2": "zieme4647",
                             "3": "5667",
                             "4": "19/06/2019:17:20:49 -0400",
                             "5": "GET",
                             "6": "/embrace/supply-chains/dynamic/vertical",
                             "7": "201",
                             "8": "20574",
            }))
        }

        single_match {
            args: func_args! [
                value: "first group and second group",
                pattern: Regex::new(r#"(?P<number>.*?) group"#).unwrap()
            ],
            want: Ok(value!({"number": "first",
                             "0": "first group",
                             "1": "first"
            }))
        }

        no_match {
            args: func_args! [
                value: "I don't match",
                pattern: Regex::new(r#"^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$"#)
                            .unwrap()
            ],
            want: Err("function call error: unable to parse regular expression".to_string()),
        }
    ];
}
