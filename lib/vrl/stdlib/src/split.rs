use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Split;

impl Function for Split {
    fn identifier(&self) -> &'static str {
        "split"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "pattern",
                kind: kind::BYTES | kind::REGEX,
                required: true,
            },
            Parameter {
                keyword: "limit",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "split string",
                source: r#"split("foobar", "b")"#,
                result: Ok(r#"["foo", "ar"]"#),
            },
            Example {
                title: "split once",
                source: r#"split("foobarbaz", "ba", 2)"#,
                result: Ok(r#"["foo", "rbaz"]"#),
            },
            Example {
                title: "split regex",
                source: r#"split("barbaz", r'ba')"#,
                result: Ok(r#"["", "r", "z"]"#),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");
        let limit = arguments.optional("limit").unwrap_or(expr!(999999999));

        Ok(Box::new(SplitFn {
            value,
            pattern,
            limit,
        }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SplitFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
    limit: Box<dyn Expression>,
}

impl Expression for SplitFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.unwrap_bytes_utf8_lossy();
        let limit = self.limit.resolve(ctx)?.unwrap_integer() as usize;

        self.pattern.resolve(ctx).and_then(|pattern| match pattern {
            Value::Regex(pattern) => Ok(pattern
                .splitn(string.as_ref(), limit as usize)
                .collect::<Vec<_>>()
                .into()),
            Value::Bytes(bytes) => {
                let pattern = String::from_utf8_lossy(&bytes);

                Ok(string
                    .splitn(limit, pattern.as_ref())
                    .collect::<Vec<_>>()
                    .into())
            }
            _ => unreachable!(),
        })
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .infallible()
            .array_mapped::<(), Kind>(map! {(): Kind::Bytes})
    }
}

// #[cfg(test)]
// #[allow(clippy::trivial_regex)]
// mod test {
//     use super::*;

//     vrl::test_type_def![
//         infallible {
//             expr: |_| SplitFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
//                 limit: None,
//             },
//             def: TypeDef {
//                 kind: value::Kind::Array,
//                 ..Default::default()
//             },
//         }

//         value_fallible {
//             expr: |_| SplitFn {
//                 value: Literal::from(10).boxed(),
//                 pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
//                 limit: None,
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: value::Kind::Array,
//                 ..Default::default()
//             },
//         }

//         pattern_expression_infallible {
//             expr: |_| SplitFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from("foo").boxed(),
//                 limit: None,
//             },
//             def: TypeDef {
//                 kind: value::Kind::Array,
//                 ..Default::default()
//             },
//         }

//         pattern_expression_fallible {
//             expr: |_| SplitFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from(10).boxed(),
//                 limit: None,
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: value::Kind::Array,
//                 ..Default::default()
//             },
//         }

//         limit_infallible {
//             expr: |_| SplitFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
//                 limit: Some(Literal::from(10).boxed()),
//             },
//             def: TypeDef {
//                 kind: value::Kind::Array,
//                 ..Default::default()
//             },
//         }

//         limit_fallible {
//             expr: |_| SplitFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
//                 limit: Some(Literal::from("foo").boxed()),
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: value::Kind::Array,
//                 ..Default::default()
//             },
//         }
//     ];
// }
