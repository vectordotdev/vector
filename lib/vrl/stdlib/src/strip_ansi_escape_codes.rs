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

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
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
        let bytes = self.value.resolve(ctx)?;
        let bytes = bytes.borrow();
        let bytes = bytes.try_bytes()?;

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

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        strip_ansi_escape_codes => StripAnsiEscapeCodes;

        no_codes {
            args: func_args![value: "foo bar"],
            want: Ok("foo bar"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        strip_1 {
            args: func_args![value: "\x1b[3;4Hfoo bar"],
            want: Ok("foo bar"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        strip_2 {
            args: func_args![value: "\x1b[46mfoo\x1b[0m bar"],
            want: Ok("foo bar"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        strip_3 {
            args: func_args![value: "\x1b[=3lfoo bar"],
            want: Ok("foo bar"),
            tdef: TypeDef::new().infallible().bytes(),
        }
    ];
}
