use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Match;

impl Function for Match {
    fn identifier(&self) -> &'static str {
        "match"
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
                kind: kind::REGEX,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "match",
                source: r#"match("foobar", r'foo')"#,
                result: Ok("true"),
            },
            Example {
                title: "mismatch",
                source: r#"match("bazqux", r'foo')"#,
                result: Ok("false"),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");

        Ok(Box::new(MatchFn { value, pattern }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MatchFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
}

impl Expression for MatchFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.unwrap_bytes_utf8_lossy();

        let pattern = self.pattern.resolve(ctx)?.unwrap_regex();

        Ok(pattern.is_match(&string).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

// #[cfg(test)]
// #[allow(clippy::trivial_regex)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![
//         value_string {
//             expr: |_| MatchFn {
//                 value: Literal::from("foo").boxed(),
//                 pattern: Regex::new("").unwrap(),
//             },
//             def: TypeDef { kind: Kind::Boolean, ..Default::default() },
//         }

//         value_non_string {
//             expr: |_| MatchFn {
//                 value: Literal::from(1).boxed(),
//                 pattern: Regex::new("").unwrap(),
//             },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }

//         value_optional {
//             expr: |_| MatchFn {
//                 value: Box::new(Noop),
//                 pattern: Regex::new("").unwrap(),
//             },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn r#match() {
//         let cases = vec![
//             (
//                 map!["foo": "foobar"],
//                 Ok(false.into()),
//                 MatchFn::new(Box::new(Path::from("foo")), Regex::new("\\s\\w+").unwrap()),
//             ),
//             (
//                 map!["foo": "foo 2 bar"],
//                 Ok(true.into()),
//                 MatchFn::new(
//                     Box::new(Path::from("foo")),
//                     Regex::new("foo \\d bar").unwrap(),
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
