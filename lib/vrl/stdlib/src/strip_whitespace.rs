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
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "start whitespace",
                source: r#"strip_whitespace("  foobar")"#,
                result: Ok("foobar"),
            },
            Example {
                title: "end whitespace",
                source: r#"strip_whitespace("foo bar  ")"#,
                result: Ok("foo bar"),
            },
            Example {
                title: "newlines",
                source: r#"strip_whitespace("\n\nfoo bar\n  ")"#,
                result: Ok("foo bar"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(StripWhitespaceFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");

        Ok(value.try_bytes_utf8_lossy()?.trim().into())
    }
}

#[derive(Debug, Clone)]
struct StripWhitespaceFn {
    value: Box<dyn Expression>,
}

impl Expression for StripWhitespaceFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        Ok(value.try_bytes_utf8_lossy()?.trim().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        strip_whitespace => StripWhitespace;

        empty {
            args: func_args![value: ""],
            want: Ok(""),
            tdef: TypeDef::new().infallible().bytes(),
        }

        just_spaces {
            args: func_args![value: "      "],
            want: Ok(""),
            tdef: TypeDef::new().infallible().bytes(),
        }

        no_spaces {
            args: func_args![value: "hi there"],
            want: Ok("hi there"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        spaces {
            args: func_args![value: "           hi there        "],
            want: Ok("hi there"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        unicode_whitespace {
            args: func_args![value: " \u{3000}\u{205F}\u{202F}\u{A0}\u{9} ❤❤ hi there ❤❤  \u{9}\u{A0}\u{202F}\u{205F}\u{3000} "],
            want: Ok("❤❤ hi there ❤❤"),
            tdef: TypeDef::new().infallible().bytes(),
        }
    ];
}
