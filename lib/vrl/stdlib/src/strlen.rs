use ::value::Value;
use vrl::prelude::*;

fn strlen(value: Value) -> Result<Value> {
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

    fn call_by_vm(&self, _ctx: &Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        strlen(value)
    }
}

#[derive(Debug, Clone)]
struct StrlenFn {
    value: Box<dyn Expression>,
}

impl Expression for StrlenFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?.into_owned();

        strlen(value).map(Cow::Owned)
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
