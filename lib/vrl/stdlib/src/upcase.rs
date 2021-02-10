use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Upcase;

impl Function for Upcase {
    fn identifier(&self) -> &'static str {
        "upcase"
    }

    fn summary(&self) -> &'static str {
        "return the uppercase variant of a string"
    }

    fn usage(&self) -> &'static str {
        indoc! {r#"
            Returns a copy of `value` that is entirely uppercase.

            "Uppercase" is defined according to the terms of the Unicode Derived Core Property
            Uppercase.
        "#}
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

#[derive(Debug)]
struct UpcaseFn {
    value: Box<dyn Expression>,
}

impl Expression for UpcaseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.unwrap_bytes();

        Ok(String::from_utf8_lossy(&bytes).to_uppercase().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().bytes().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use std::convert::TryFrom;

    vrl::test_type_def![
        string {
            expr: |_| UpcaseFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        non_string {
            expr: |_| UpcaseFn { value: Literal::from(true).boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }
    ];
}
