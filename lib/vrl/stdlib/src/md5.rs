use ::value::Value;
use md5::Digest;
use vrl::prelude::*;

fn md5(value: Value) -> Result<Value> {
    let value = value.try_bytes()?;
    Ok(hex::encode(md5::Md5::digest(&value)).into())
}

#[derive(Clone, Copy, Debug)]
pub struct Md5;

impl Function for Md5 {
    fn identifier(&self) -> &'static str {
        "md5"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "md5",
            source: r#"md5("foobar")"#,
            result: Ok("3858f62230ac3c915f300c664312c63f"),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(Md5Fn { value }))
    }

    fn call_by_vm(&self, _ctx: &Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        md5(value)
    }
}

#[derive(Debug, Clone)]
struct Md5Fn {
    value: Box<dyn Expression>,
}

impl Expression for Md5Fn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?.into_owned();
        md5(value).map(Cow::Owned)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        md5 => Md5;

        md5 {
            args: func_args![value: "foo"],
            want: Ok(value!("acbd18db4cc2f85cedef654fccc4a4d8")),
            tdef: TypeDef::bytes().infallible(),
        }
    ];
}
