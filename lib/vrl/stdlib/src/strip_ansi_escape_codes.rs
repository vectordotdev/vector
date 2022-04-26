use bytes::Bytes;
use vrl::prelude::*;

fn strip_ansi_escape_codes(bytes: Value) -> Resolved {
    let bytes = bytes.try_bytes()?;
    strip_ansi_escapes::strip(&bytes)
        .map(Bytes::from)
        .map(Value::from)
        .map(Into::into)
        .map_err(|e| e.to_string().into())
}

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
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(StripAnsiEscapeCodesFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        strip_ansi_escape_codes(value)
    }
}

#[derive(Debug, Clone)]
struct StripAnsiEscapeCodesFn {
    value: Box<dyn Expression>,
}

impl Expression for StripAnsiEscapeCodesFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;

        strip_ansi_escape_codes(bytes)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        // We're marking this as infallible, because `strip_ansi_escapes` only
        // fails if it can't write to the buffer, which is highly unlikely to
        // occur.
        TypeDef::bytes().infallible()
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
            tdef: TypeDef::bytes().infallible(),
        }

        strip_1 {
            args: func_args![value: "\x1b[3;4Hfoo bar"],
            want: Ok("foo bar"),
            tdef: TypeDef::bytes().infallible(),
        }

        strip_2 {
            args: func_args![value: "\x1b[46mfoo\x1b[0m bar"],
            want: Ok("foo bar"),
            tdef: TypeDef::bytes().infallible(),
        }

        strip_3 {
            args: func_args![value: "\x1b[=3lfoo bar"],
            want: Ok("foo bar"),
            tdef: TypeDef::bytes().infallible(),
        }
    ];
}
