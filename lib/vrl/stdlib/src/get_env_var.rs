use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct GetEnvVar;

impl Function for GetEnvVar {
    fn identifier(&self) -> &'static str {
        "get_env_var"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "name",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "home",
            source: r#"get_env_var!("HOME") != """#,
            result: Ok("true"),
        }]
    }

    fn compile(&self, _state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let name = arguments.required("name");

        Ok(Box::new(GetEnvVarFn { name }))
    }
}

#[derive(Debug, Clone)]
struct GetEnvVarFn {
    name: Box<dyn Expression>,
}

impl Expression for GetEnvVarFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.name.resolve(ctx)?;
        let name = value.try_bytes_utf8_lossy()?;

        std::env::var(name.as_ref())
            .map(Into::into)
            .map_err(|e| e.to_string().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        get_env_var => GetEnvVar;

        before_each => {
            std::env::set_var("VAR2", "var");
        }

        doesnt_exist {
            args: func_args![name: "VAR1"],
            want: Err("environment variable not found"),
            tdef: TypeDef::new().fallible().bytes(),
        }

        exists {
            args: func_args![name: "VAR2"],
            want: Ok(value!("var")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        invalid1 {
            args: func_args![name: "="],
            want: Err("environment variable not found"),
            tdef: TypeDef::new().fallible().bytes(),
        }

        invalid2 {
            args: func_args![name: ""],
            want: Err("environment variable not found"),
            tdef: TypeDef::new().fallible().bytes(),
        }

        invalid3 {
            args: func_args![name: "a=b"],
            want: Err("environment variable not found"),
            tdef: TypeDef::new().fallible().bytes(),
        }
    ];
}
