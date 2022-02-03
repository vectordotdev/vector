use vrl::prelude::*;

fn upcase(value: Value) -> std::result::Result<Value, ExpressionError> {
    Ok(value.try_bytes_utf8_lossy()?.to_uppercase().into())
}

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

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(UpcaseFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        upcase(value)
    }
}

#[derive(Debug, Clone)]
struct UpcaseFn {
    value: Box<dyn Expression>,
}

impl Expression for UpcaseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        upcase(value)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        upcase => Upcase;

        simple {
            args: func_args![value: "FOO 2 bar"],
            want: Ok(value!("FOO 2 BAR")),
            tdef: TypeDef::new().bytes(),
        }
    ];
}
