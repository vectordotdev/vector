use bytes::Bytes;
use remap::prelude::*;

#[derive(Debug)]
pub struct StripAnsiEscapeCodes;

impl Function for StripAnsiEscapeCodes {
    fn identifier(&self) -> &'static str {
        "strip_ansi_escape_codes"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::String(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;

        Ok(Box::new(StripAnsiEscapeCodesFn { value }))
    }
}

#[derive(Debug)]
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
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let bytes = required!(state, object, self.value, Value::String(v) => v);

        strip_ansi_escapes::strip(&bytes)
            .map(Bytes::from)
            .map(Value::from)
            .map(Into::into)
            .map_err(|e| e.to_string().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn strip_ansi_escape_codes() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                StripAnsiEscapeCodesFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map![],
                Ok(Some("foo bar".into())),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from("foo bar"))),
            ),
            (
                map![],
                Ok(Some("foo bar".into())),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from("\x1b[3;4Hfoo bar"))),
            ),
            (
                map![],
                Ok(Some("foo bar".into())),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from("\x1b[46mfoo\x1b[0m bar"))),
            ),
            (
                map![],
                Ok(Some("foo bar".into())),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from("\x1b[=3lfoo bar"))),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
