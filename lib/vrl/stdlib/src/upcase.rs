use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Upcase;

impl Function for Upcase {
    fn identifier(&self) -> &'static str {
        "upcase"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "upcase",
            source: r#"upcase("foo 2 bar")"#,
            result: Ok("FOO 2 BAR"),
        }]
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(UpcaseFn { value }))
    }
}

#[derive(Debug, Clone)]
struct UpcaseFn {
    value: Box<dyn Expression>,
}

impl Expression for UpcaseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        Ok(value.unwrap_bytes_utf8_lossy().to_uppercase().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;
//     use std::convert::TryFrom;

//     vrl::test_type_def![
//         string {
//             expr: |_| UpcaseFn { value: Literal::from("foo").boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         non_string {
//             expr: |_| UpcaseFn { value: Literal::from(true).boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Bytes,
//                 ..Default::default()
//             },
//         }
//     ];
// }
