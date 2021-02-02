use regex::Regex;
use remap::prelude::*;

use crate::util;

#[derive(Clone, Copy, Debug)]
pub struct ParseRegexAll;

impl Function for ParseRegexAll {
    fn identifier(&self) -> &'static str {
        "parse_regex_all"
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

        Ok(Box::new(ParseRegexAllFn { value, pattern }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParseRegexAllFn {
    value: Box<dyn Expression>,
    pattern: Regex,
}

impl Expression for ParseRegexAllFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        Ok(self
            .pattern
            .captures_iter(&value)
            .map(|capture| util::capture_regex_to_map(&self.pattern, capture).into())
            .collect::<Vec<Value>>()
            .into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_inner_type(Some(inner_type_def!([TypeDef::from(value::Kind::Map)
                .with_inner_type(Some(util::regex_type_def(&self.pattern)))])))
            .with_constraint(value::Kind::Array)
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use super::*;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| ParseRegexAllFn {
                value: Literal::from("foo").boxed(),
                pattern: Regex::new("^(?P<group>.*)$").unwrap(),
            },
            def: TypeDef { kind: Kind::Array,
                           inner_type_def: Some(inner_type_def!([ TypeDef::from(Kind::Map)
                                                                  .with_inner_type(Some(inner_type_def! ({ "0": Kind::Bytes,
                                                                                                           "1": Kind::Bytes,
                                                                                                           "group": Kind::Bytes
                                                                  }))) ])),
                           ..Default::default() },
        }

        value_non_string {
            expr: |_| ParseRegexAllFn {
                value: Literal::from(1).boxed(),
                pattern: Regex::new("^(?P<group>.*)$").unwrap(),
            },
            def: TypeDef { fallible: true,
                           kind: Kind::Array,
                           inner_type_def: Some(inner_type_def!([ TypeDef::from(Kind::Map)
                                                                  .with_inner_type(Some(inner_type_def! ({ "0": Kind::Bytes,
                                                                                                           "1": Kind::Bytes,
                                                                                                           "group": Kind::Bytes
                                                                  }))) ])),
            },
        }

        value_optional {
            expr: |_| ParseRegexAllFn {
                value: Box::new(Noop),
                pattern: Regex::new("^(?P<group>.*)$").unwrap(),
            },
            def: TypeDef { fallible: true,
                           kind: Kind::Array,
                           inner_type_def: Some(inner_type_def!([ TypeDef::from(Kind::Map)
                                                                  .with_inner_type(Some(inner_type_def! ({ "0": Kind::Bytes,
                                                                                                           "1": Kind::Bytes,
                                                                                                           "group": Kind::Bytes
                                                                  }))) ])),
            },
        }
    ];

    test_function![
        find_all => ParseRegexAll;

        matches {
            args: func_args![
                value: "apples and carrots, peaches and peas",
                pattern: Regex::new(r#"(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)"#).unwrap()
            ],
            want: Ok(value!([{"fruit": "apples",
                              "veg": "carrots",
                              "0": "apples and carrots",
                              "1": "apples",
                              "2": "carrots"},
                             {"fruit": "peaches",
                              "veg": "peas",
                              "0": "peaches and peas",
                              "1": "peaches",
                              "2": "peas"}]))
        }

        no_matches {
            args: func_args![
                value: "I don't match",
                pattern: Regex::new(r#"(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)"#).unwrap()
            ],
            want: Ok(value!([]))
        }
    ];
}
