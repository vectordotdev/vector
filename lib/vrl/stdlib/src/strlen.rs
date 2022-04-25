use vrl::prelude::*;

fn strlen(value: Value) -> Resolved {
    let v = value.try_bytes()?;

    Ok(String::from_utf8_lossy(&v).chars().count().into())
}

#[derive(Clone, Copy, Debug)]
pub struct Strlen;

impl Function for Strlen {
    fn identifier(&self) -> &'static str {
        "strlen"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Characters",
            source: r#"strlen("ñandú")"#,
            result: Ok("5"),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(StrlenFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        strlen(value)
    }
}

#[derive(Debug, Clone)]
struct StrlenFn {
    value: Box<dyn Expression>,
}

impl Expression for StrlenFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        strlen(value)
    }

    fn type_def(&self, _state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::integer().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        strlen => Strlen;

        string_value {
            args: func_args![value: value!("ñandú")],
            want: Ok(value!(5)),
            tdef: TypeDef::integer().infallible(),
        }
    ];
}
