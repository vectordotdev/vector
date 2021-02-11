use regex::Regex;
use vrl::prelude::*;

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
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "pattern",
                kind: kind::ANY,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required_regex("pattern")?;

        Ok(Box::new(ParseRegexAllFn { value, pattern }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Simple match",
            source: r#"parse_regex_all!("apples and carrots, peaches and peas", r'(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)')"#,
            result: Ok(indoc! { r#"[
               {"fruit": "apples",
                "veg": "carrots",
                "0": "apples and carrots",
                "1": "apples",
                "2": "carrots"},
               {"fruit": "peaches",
                "veg": "peas",
                "0": "peaches and peas",
                "1": "peaches",
                "2": "peas"}]"# }),
        }]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParseRegexAllFn {
    value: Box<dyn Expression>,
    pattern: Regex,
}

impl Expression for ParseRegexAllFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        Ok(self
            .pattern
            .captures_iter(&value)
            .map(|capture| util::capture_regex_to_map(&self.pattern, capture).into())
            .collect::<Vec<Value>>()
            .into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        let inner_type_def = TypeDef::new()
            .object(util::regex_type_def(&self.pattern))
            .add_null();

        TypeDef::new()
            .fallible()
            .array_mapped::<(), TypeDef>(map![(): inner_type_def])
    }
}

/*
#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use super::*;

    vrl::test_type_def![
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
*/
