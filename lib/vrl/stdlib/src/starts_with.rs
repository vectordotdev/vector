use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct StartsWith;

impl Function for StartsWith {
    fn identifier(&self) -> &'static str {
        "starts_with"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "substring",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "case_sensitive",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "match",
                source: r#"starts_with("foobar", "foo")"#,
                result: Ok("true"),
            },
            Example {
                title: "mismatch",
                source: r#"starts_with("foobar", "baz")"#,
                result: Ok("false"),
            },
            Example {
                title: "case sensitive",
                source: r#"starts_with("foobar", "F", true)"#,
                result: Ok("false"),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let substring = arguments.required("substring");
        let case_sensitive = arguments.optional("case_sensitive").unwrap_or(expr!(false));

        Ok(Box::new(StartsWithFn {
            value,
            substring,
            case_sensitive,
        }))
    }
}

#[derive(Debug, Clone)]
struct StartsWithFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Box<dyn Expression>,
}

impl Expression for StartsWithFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let case_sensitive = self.case_sensitive.resolve(ctx)?.unwrap_boolean();

        let substring = {
            let value = self.substring.resolve(ctx)?;
            let string = value.unwrap_bytes_utf8_lossy();

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        let value = {
            let value = self.value.resolve(ctx)?;
            let string = value.unwrap_bytes_utf8_lossy();

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        Ok(value.starts_with(&substring).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![
//         value_string {
//             expr: |_| StartsWithFn {
//                 value: Literal::from("foo").boxed(),
//                 substring: Literal::from("foo").boxed(),
//                 case_sensitive: None,
//             },
//             def: TypeDef { kind: Kind::Boolean, ..Default::default() },
//         }

//         value_non_string {
//             expr: |_| StartsWithFn {
//                 value: Literal::from(true).boxed(),
//                 substring: Literal::from("foo").boxed(),
//                 case_sensitive: None,
//             },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }

//         substring_non_string {
//             expr: |_| StartsWithFn {
//                 value: Literal::from("foo").boxed(),
//                 substring: Literal::from(true).boxed(),
//                 case_sensitive: None,
//             },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }

//         case_sensitive_non_boolean {
//             expr: |_| StartsWithFn {
//                 value: Literal::from("foo").boxed(),
//                 substring: Literal::from("foo").boxed(),
//                 case_sensitive: Some(Literal::from(1).boxed()),
//             },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn starts_with() {
//         let cases = vec![
//             (
//                 map![],
//                 Ok(false.into()),
//                 StartsWithFn::new(Box::new(Literal::from("foo")), "bar", false),
//             ),
//             (
//                 map![],
//                 Ok(false.into()),
//                 StartsWithFn::new(Box::new(Literal::from("foo")), "foobar", false),
//             ),
//             (
//                 map![],
//                 Ok(true.into()),
//                 StartsWithFn::new(Box::new(Literal::from("foo")), "foo", false),
//             ),
//             (
//                 map![],
//                 Ok(false.into()),
//                 StartsWithFn::new(Box::new(Literal::from("foobar")), "oba", false),
//             ),
//             (
//                 map![],
//                 Ok(true.into()),
//                 StartsWithFn::new(Box::new(Literal::from("foobar")), "foo", false),
//             ),
//             (
//                 map![],
//                 Ok(false.into()),
//                 StartsWithFn::new(Box::new(Literal::from("foobar")), "bar", false),
//             ),
//             (
//                 map![],
//                 Ok(true.into()),
//                 StartsWithFn::new(Box::new(Literal::from("FOObar")), "FOO", true),
//             ),
//             (
//                 map![],
//                 Ok(false.into()),
//                 StartsWithFn::new(Box::new(Literal::from("foobar")), "FOO", true),
//             ),
//             (
//                 map![],
//                 Ok(true.into()),
//                 StartsWithFn::new(Box::new(Literal::from("foobar")), "FOO", false),
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
