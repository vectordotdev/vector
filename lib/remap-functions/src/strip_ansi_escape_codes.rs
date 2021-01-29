use bytes::Bytes;
use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct StripAnsiEscapeCodes;

impl Function for StripAnsiEscapeCodes {
    fn identifier(&self) -> &'static str {
        "strip_ansi_escape_codes"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(StripAnsiEscapeCodesFn { value }))
    }
}

#[derive(Debug, Clone)]
struct StripAnsiEscapeCodesFn {
    value: Box<dyn Expression>,
}

impl StripAnsiEscapeCodesFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for StripAnsiEscapeCodesFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;

        strip_ansi_escapes::strip(&bytes)
            .map(Bytes::from)
            .map(Value::from)
            .map(Into::into)
            .map_err(|e| e.to_string().into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            // TODO: Can probably remove this, as it only fails if writing to
            //       the buffer fails.
            .into_fallible(true)
            .with_constraint(value::Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    remap::test_type_def![
        value_string {
            expr: |_| StripAnsiEscapeCodesFn { value: Literal::from("foo").boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Bytes, ..Default::default() },
        }

        fallible_expression {
            expr: |_| StripAnsiEscapeCodesFn { value: Literal::from(10).boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Bytes, ..Default::default() },
        }
    ];

    #[test]
    fn strip_ansi_escape_codes() {
        let cases = vec![
            (
                btreemap![],
                Ok("foo bar".into()),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from("foo bar"))),
            ),
            (
                btreemap![],
                Ok("foo bar".into()),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from("\x1b[3;4Hfoo bar"))),
            ),
            (
                btreemap![],
                Ok("foo bar".into()),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from("\x1b[46mfoo\x1b[0m bar"))),
            ),
            (
                btreemap![],
                Ok("foo bar".into()),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from("\x1b[=3lfoo bar"))),
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
