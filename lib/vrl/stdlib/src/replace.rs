use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Replace;

impl Function for Replace {
    fn identifier(&self) -> &'static str {
        "replace"
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
                keyword: "with",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "count",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "replace all",
                source: r#"replace("foobar", "o", "i")"#,
                result: Ok("fiibar"),
            },
            Example {
                title: "replace count",
                source: r#"replace("foobar", "o", "i", count: 1)"#,
                result: Ok("fiobar"),
            },
            Example {
                title: "replace regex",
                source: r#"replace("foobar", r'o|a', "i")"#,
                result: Ok("fiibir"),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");
        let with = arguments.required("with");
        let count = arguments.optional("count").unwrap_or(expr!(-1));

        Ok(Box::new(ReplaceFn {
            value,
            pattern,
            with,
            count,
        }))
    }
}

#[derive(Debug, Clone)]
struct ReplaceFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
    with: Box<dyn Expression>,
    count: Box<dyn Expression>,
}

impl Expression for ReplaceFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let value = value.unwrap_bytes_utf8_lossy();

        let with_value = self.with.resolve(ctx)?;
        let with = with_value.unwrap_bytes_utf8_lossy();

        let count = self.count.resolve(ctx)?.unwrap_integer();

        self.pattern.resolve(ctx).and_then(|pattern| match pattern {
            Value::Bytes(bytes) => {
                let pattern = String::from_utf8_lossy(&bytes);
                let replaced = match count {
                    i if i > 0 => value.replacen(pattern.as_ref(), &with, i as usize),
                    i if i < 0 => value.replace(pattern.as_ref(), &with),
                    _ => value.into_owned(),
                };

                Ok(replaced.into())
            }
            Value::Regex(regex) => {
                let replaced = match count {
                    i if i > 0 => regex
                        .replacen(&value, i as usize, with.as_ref())
                        .as_bytes()
                        .into(),
                    i if i < 0 => regex.replace_all(&value, with.as_ref()).as_bytes().into(),
                    _ => value.into(),
                };

                Ok(replaced)
            }
            _ => unreachable!("argument-type invariant"),
        })
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

// #[cfg(test)]
// #[allow(clippy::trivial_regex)]
// mod test {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![
//         infallible {
//             expr: |_| ReplaceFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
//                 with: Literal::from("foo").boxed(),
//                 count: None,
//             },
//             def: TypeDef {
//                 kind: value::Kind::Bytes,
//                 ..Default::default()
//             },
//         }

//         value_fallible {
//             expr: |_| ReplaceFn {
//                 value: Literal::from(10).boxed(),
//                 pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
//                 with: Literal::from("foo").boxed(),
//                 count: None,
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: value::Kind::Bytes,
//                 ..Default::default()
//             },
//         }

//         pattern_expression_infallible {
//             expr: |_| ReplaceFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from("foo").boxed(),
//                 with: Literal::from("foo").boxed(),
//                 count: None,
//             },
//             def: TypeDef {
//                 kind: value::Kind::Bytes,
//                 ..Default::default()
//             },
//         }

//         pattern_expression_fallible {
//             expr: |_| ReplaceFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from(10).boxed(),
//                 with: Literal::from("foo").boxed(),
//                 count: None,
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: value::Kind::Bytes,
//                 ..Default::default()
//             },
//         }

//         with_fallible {
//             expr: |_| ReplaceFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
//                 with: Literal::from(10).boxed(),
//                 count: None,
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: value::Kind::Bytes,
//                 ..Default::default()
//             },
//         }

//         count_infallible {
//             expr: |_| ReplaceFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
//                 with: Literal::from("foo").boxed(),
//                 count: Some(Literal::from(10).boxed()),
//             },
//             def: TypeDef {
//                 kind: value::Kind::Bytes,
//                 ..Default::default()
//             },
//         }

//         count_fallible {
//             expr: |_| ReplaceFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
//                 with: Literal::from("foo").boxed(),
//                 count: Some(Literal::from("foo").boxed()),
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: value::Kind::Bytes,
//                 ..Default::default()
//             },
//         }
//     ];

//     #[test]
//     fn check_replace_string() {
//         let cases = vec![
//             (
//                 map![],
//                 Ok("I like opples ond bononos".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from("a").boxed(),
//                     "o",
//                     None,
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("I like opples ond bononos".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from("a").boxed(),
//                     "o",
//                     Some(-1),
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("I like apples and bananas".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from("a").boxed(),
//                     "o",
//                     Some(0),
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("I like opples and bananas".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from("a").boxed(),
//                     "o",
//                     Some(1),
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("I like opples ond bananas".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from("a").boxed(),
//                     "o",
//                     Some(2),
//                 ),
//             ),
//         ];

//         let mut state = state::Program::default();

//         for (object, exp, func) in cases {
//             let mut object: Value = object.into();
//             let got = func
//                 .resolve(&mut ctx)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }

//     #[test]
//     fn check_replace_regex() {
//         let cases = vec![
//             (
//                 map![],
//                 Ok("I like opples ond bononos".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from(regex::Regex::new("a").unwrap()).boxed(),
//                     "o",
//                     None,
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("I like opples ond bononos".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from(regex::Regex::new("a").unwrap()).boxed(),
//                     "o",
//                     Some(-1),
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("I like apples and bananas".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from(regex::Regex::new("a").unwrap()).boxed(),
//                     "o",
//                     Some(0),
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("I like opples and bananas".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from(regex::Regex::new("a").unwrap()).boxed(),
//                     "o",
//                     Some(1),
//                 ),
//             ),
//             (
//                 map![],
//                 Ok("I like opples ond bananas".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from(regex::Regex::new("a").unwrap()).boxed(),
//                     "o",
//                     Some(2),
//                 ),
//             ),
//         ];

//         let mut state = state::Program::default();

//         for (object, exp, func) in cases {
//             let mut object: Value = object.into();
//             let got = func
//                 .resolve(&mut ctx)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }

//     #[test]
//     fn check_replace_other() {
//         let cases = vec![
//             (
//                 map![],
//                 Ok("I like biscuits and bananas".into()),
//                 ReplaceFn::new(
//                     Literal::from("I like apples and bananas").boxed(),
//                     Literal::from("apples").boxed(),
//                     "biscuits",
//                     None,
//                 ),
//             ),
//             (
//                 map!["foo": "I like apples and bananas"],
//                 Ok("I like opples and bananas".into()),
//                 ReplaceFn::new(
//                     Box::new(Path::from("foo")),
//                     Literal::from(regex::Regex::new("a").unwrap()).boxed(),
//                     "o",
//                     Some(1),
//                 ),
//             ),
//             (
//                 map!["foo": "I like [apples] and bananas"],
//                 Ok("I like biscuits and bananas".into()),
//                 ReplaceFn::new(
//                     Box::new(Path::from("foo")),
//                     Literal::from(regex::Regex::new("\\[apples\\]").unwrap()).boxed(),
//                     "biscuits",
//                     None,
//                 ),
//             ),
//         ];

//         let mut state = state::Program::default();

//         for (object, exp, func) in cases {
//             let mut object: Value = object.into();
//             let got = func
//                 .resolve(&mut ctx)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }
// }
