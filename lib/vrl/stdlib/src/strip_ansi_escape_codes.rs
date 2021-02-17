use bytes::Bytes;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct StripAnsiEscapeCodes;

impl Function for StripAnsiEscapeCodes {
    fn identifier(&self) -> &'static str {
        "strip_ansi_escape_codes"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(StripAnsiEscapeCodesFn { value }))
    }
}

#[derive(Debug, Clone)]
struct StripAnsiEscapeCodesFn {
    value: Box<dyn Expression>,
}

impl Expression for StripAnsiEscapeCodesFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.unwrap_bytes();

        strip_ansi_escapes::strip(&bytes)
            .map(Bytes::from)
            .map(Value::from)
            .map(Into::into)
            .map_err(|e| e.to_string().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        // We're marking this as infallible, because `strip_ansi_escapes` only
        // fails if it can't write to the buffer, which is highly unlikely to
        // occur.
        TypeDef::new().infallible().bytes()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![
//         value_string {
//             expr: |_| StripAnsiEscapeCodesFn { value: Literal::from("foo").boxed() },
//             def: TypeDef { fallible: true, kind: value::Kind::Bytes, ..Default::default() },
//         }

//         fallible_expression {
//             expr: |_| StripAnsiEscapeCodesFn { value: Literal::from(10).boxed() },
//             def: TypeDef { fallible: true, kind: value::Kind::Bytes, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn strip_ansi_escape_codes() {
//         let cases = vec![
//             (
//                 map![],
//                 Ok("foo bar".into()),
//                 StripAnsiEscapeCodesFn::new(Box::new(Literal::from("foo bar"))),
//             ),
//             (
//                 map![],
//                 Ok("foo bar".into()),
//                 StripAnsiEscapeCodesFn::new(Box::new(Literal::from("\x1b[3;4Hfoo bar"))),
//             ),
//             (
//                 map![],
//                 Ok("foo bar".into()),
//                 StripAnsiEscapeCodesFn::new(Box::new(Literal::from("\x1b[46mfoo\x1b[0m bar"))),
//             ),
//             (
//                 map![],
//                 Ok("foo bar".into()),
//                 StripAnsiEscapeCodesFn::new(Box::new(Literal::from("\x1b[=3lfoo bar"))),
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
