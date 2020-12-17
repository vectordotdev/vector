use regex::Regex;
use remap::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct Find;

impl Function for Find {
    fn identifier(&self) -> &'static str {
        "find"
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

        Ok(Box::new(FindFn { value, pattern }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FindFn {
    value: Box<dyn Expression>,
    pattern: Regex,
}

impl FindFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, pattern: Regex) -> Self {
        Self { value, pattern }
    }
}

impl Expression for FindFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        Ok(self
            .pattern
            .captures(&value)
            .map(|capture| {
                self.pattern
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
            })
            .unwrap_or_else(BTreeMap::new)
            .into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Map)
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use super::*;
    use crate::map;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| FindFn {
                value: Literal::from("foo").boxed(),
                pattern: Regex::new("").unwrap(),
            },
            def: TypeDef { kind: Kind::Map, ..Default::default() },
        }

        value_non_string {
            expr: |_| FindFn {
                value: Literal::from(1).boxed(),
                pattern: Regex::new("").unwrap(),
            },
            def: TypeDef { fallible: true, kind: Kind::Map, ..Default::default() },
        }

        value_optional {
            expr: |_| FindFn {
                value: Box::new(Noop),
                pattern: Regex::new("").unwrap(),
            },
            def: TypeDef { fallible: true, kind: Kind::Map, ..Default::default() },
        }
    ];

    #[test]
    fn find() {
        let cases = vec![
            (
                map!["message": "5.86.210.12 - zieme4647 5667 [19/06/2019:17:20:49 -0400] \"GET /embrace/supply-chains/dynamic/vertical\" 201 20574"],
                Ok(map!["bytes_in": "5667",
                        "host": "5.86.210.12",
                        "user": "zieme4647",
                        "timestamp": "19/06/2019:17:20:49 -0400",
                        "method": "GET",
                        "path": "/embrace/supply-chains/dynamic/vertical",
                        "status": "201",
                        "bytes_out": "20574"].into()),
                FindFn::new(Box::new(Path::from("message")),
                             Regex::new(r#"^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$"#)
                             .unwrap()),
            ),
            (
                map!["message": "first group and second group"],
                Ok(map!["number": "first"].into()),
                FindFn::new(Box::new(Path::from("message")),
                             Regex::new(r#"(?P<number>.*?) group"#)
                             .unwrap()),
            ),
            (
                map!["message": "I don't match"],
                Ok(map![].into()),
                FindFn::new(Box::new(Path::from("message")),
                            Regex::new(r#"^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$"#)
                            .unwrap()),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
