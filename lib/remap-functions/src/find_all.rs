use regex::Regex;
use remap::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct FindAll;

impl Function for FindAll {
    fn identifier(&self) -> &'static str {
        "find_all"
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

        Ok(Box::new(FindAllFn { value, pattern }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FindAllFn {
    value: Box<dyn Expression>,
    pattern: Regex,
}

impl Expression for FindAllFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        Ok(self
            .pattern
            .captures_iter(&value)
            .map(|capture| {
                let val: Value = self
                    .pattern
                    .capture_names()
                    .filter_map(|name| {
                        // We only work with groups that have been given a name.
                        // `name` will be None if it has no name and thus filtered out.
                        name.map(|name| {
                            (
                                name.to_owned(),
                                capture.name(name).map(|s| s.as_str()).into(),
                            )
                        })
                    })
                    .collect::<BTreeMap<_, _>>()
                    .into();

                val
            })
            .collect::<Vec<Value>>()
            .into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
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
            expr: |_| FindAllFn {
                value: Literal::from("foo").boxed(),
                pattern: Regex::new("").unwrap(),
            },
            def: TypeDef { kind: Kind::Array, ..Default::default() },
        }

        value_non_string {
            expr: |_| FindAllFn {
                value: Literal::from(1).boxed(),
                pattern: Regex::new("").unwrap(),
            },
            def: TypeDef { fallible: true, kind: Kind::Array, ..Default::default() },
        }

        value_optional {
            expr: |_| FindAllFn {
                value: Box::new(Noop),
                pattern: Regex::new("").unwrap(),
            },
            def: TypeDef { fallible: true, kind: Kind::Array, ..Default::default() },
        }
    ];

    test_function![
        find_all => FindAll;

        matches {
            args: func_args![
                value: "apples and carrots, peaches and peas",
                pattern: Regex::new(r#"(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)"#).unwrap()
            ],
            want: Ok(value!([{"fruit": "apples",
                              "veg": "carrots"},
                             {"fruit": "peaches",
                              "veg": "peas"}]))
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
