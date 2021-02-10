use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct StripWhitespace;

impl Function for StripWhitespace {
    fn identifier(&self) -> &'static str {
        "strip_whitespace"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(StripWhitespaceFn { value }))
    }
}

#[derive(Debug)]
struct StripWhitespaceFn {
    value: Box<dyn Expression>,
}

impl StripWhitespaceFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for StripWhitespaceFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        Ok(value.trim().into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    vrl::test_type_def![
        value_string {
            expr: |_| StripWhitespaceFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: value::Kind::Bytes, ..Default::default() },
        }

        fallible_expression {
            expr: |_| StripWhitespaceFn { value: Literal::from(10).boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Bytes, ..Default::default() },
        }
    ];

    #[test]
    fn strip_whitespace() {
        let cases = vec![
            (
                map!["foo": ""],
                Ok("".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "     "],
                Ok("".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "hi there"],
                Ok("hi there".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "           hi there        "],
                Ok("hi there".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": " \u{3000}\u{205F}\u{202F}\u{A0}\u{9} ❤❤ hi there ❤❤  \u{9}\u{A0}\u{202F}\u{205F}\u{3000} "],
                Ok("❤❤ hi there ❤❤".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
