use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Truncate;

impl Function for Truncate {
    fn identifier(&self) -> &'static str {
        "truncate"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "limit",
                kind: kind::INTEGER,
                required: true,
            },
            Parameter {
                keyword: "ellipsis",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "truncate",
                source: r#"truncate("foobar", 3)"#,
                result: Ok("foo"),
            },
            Example {
                title: "too short",
                source: r#"truncate("foo", 4)"#,
                result: Ok("foo"),
            },
            Example {
                title: "ellipsis",
                source: r#"truncate("foo", 2, true)"#,
                result: Ok("fo..."),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let limit = arguments.required("limit");
        let ellipsis = arguments.optional("ellipsis").unwrap_or(expr!(false));

        Ok(Box::new(TruncateFn {
            value,
            limit,
            ellipsis,
        }))
    }
}

#[derive(Debug, Clone)]
struct TruncateFn {
    value: Box<dyn Expression>,
    limit: Box<dyn Expression>,
    ellipsis: Box<dyn Expression>,
}

impl Expression for TruncateFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let mut value = value.unwrap_bytes_utf8_lossy().into_owned();

        let limit = self.limit.resolve(ctx)?.unwrap_integer();
        let limit = if limit < 0 { 0 } else { limit as usize };

        let ellipsis = self.ellipsis.resolve(ctx)?.unwrap_boolean();

        let pos = if let Some((pos, chr)) = value.char_indices().take(limit).last() {
            // char_indices gives us the starting position of the character at limit,
            // we want the end position.
            pos + chr.len_utf8()
        } else {
            // We have an empty string
            0
        };

        if value.len() > pos {
            value.truncate(pos);

            if ellipsis {
                value.push_str("...");
            }
        }

        Ok(value.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![
//         infallible {
//             expr: |_| TruncateFn {
//                 value: Literal::from("foo").boxed(),
//                 limit: Literal::from(1).boxed(),
//                 ellipsis: None,
//             },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         value_non_string {
//             expr: |_| TruncateFn {
//                 value: Literal::from(false).boxed(),
//                 limit: Literal::from(1).boxed(),
//                 ellipsis: None,
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Bytes,
//                 ..Default::default()
//             },
//         }

//         limit_float {
//             expr: |_| TruncateFn {
//                 value: Literal::from("foo").boxed(),
//                 limit: Literal::from(1.0).boxed(),
//                 ellipsis: None,
//             },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         limit_non_number {
//             expr: |_| TruncateFn {
//                 value: Literal::from("foo").boxed(),
//                 limit: Literal::from("bar").boxed(),
//                 ellipsis: None,
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Bytes,
//                 ..Default::default()
//             },
//         }

//         ellipsis_boolean {
//             expr: |_| TruncateFn {
//                 value: Literal::from("foo").boxed(),
//                 limit: Literal::from(10).boxed(),
//                 ellipsis: Some(Literal::from(true).boxed()),
//             },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         ellipsis_non_boolean {
//             expr: |_| TruncateFn {
//                 value: Literal::from("foo").boxed(),
//                 limit: Literal::from("bar").boxed(),
//                 ellipsis: Some(Literal::from("baz").boxed()),
//             },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Bytes,
//                 ..Default::default()
//             },
//         }
//     ];

//     #[test]
//     fn truncate() {
//         let cases = vec![
//             (
//                 map!["foo": "Super"],
//                 Ok("".into()),
//                 TruncateFn::new(
//                     Box::new(Path::from("foo")),
//                     Box::new(Literal::from(0.0)),
//                     Some(false.into()),
//                 ),
//             ),
//             (
//                 map!["foo": "Super"],
//                 Ok("...".into()),
//                 TruncateFn::new(
//                     Box::new(Path::from("foo")),
//                     Box::new(Literal::from(0.0)),
//                     Some(true.into()),
//                 ),
//             ),
//             (
//                 map!["foo": "Super"],
//                 Ok("Super".into()),
//                 TruncateFn::new(
//                     Box::new(Path::from("foo")),
//                     Box::new(Literal::from(10.0)),
//                     Some(false.into()),
//                 ),
//             ),
//             (
//                 map!["foo": "Super"],
//                 Ok("Super".into()),
//                 TruncateFn::new(
//                     Box::new(Path::from("foo")),
//                     Box::new(Literal::from(5.0)),
//                     Some(true.into()),
//                 ),
//             ),
//             (
//                 map!["foo": "Supercalifragilisticexpialidocious"],
//                 Ok("Super".into()),
//                 TruncateFn::new(
//                     Box::new(Path::from("foo")),
//                     Box::new(Literal::from(5.0)),
//                     Some(false.into()),
//                 ),
//             ),
//             (
//                 map!["foo": "♔♕♖♗♘♙♚♛♜♝♞♟"],
//                 Ok("♔♕♖♗♘♙...".into()),
//                 TruncateFn::new(
//                     Box::new(Path::from("foo")),
//                     Box::new(Literal::from(6.0)),
//                     Some(true.into()),
//                 ),
//             ),
//             (
//                 map!["foo": "Supercalifragilisticexpialidocious"],
//                 Ok("Super...".into()),
//                 TruncateFn::new(
//                     Box::new(Path::from("foo")),
//                     Box::new(Literal::from(5.0)),
//                     Some(true.into()),
//                 ),
//             ),
//             (
//                 map!["foo": "Supercalifragilisticexpialidocious"],
//                 Ok("Super".into()),
//                 TruncateFn::new(
//                     Box::new(Path::from("foo")),
//                     Box::new(Literal::from(5.0)),
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
