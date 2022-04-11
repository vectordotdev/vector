use vrl::prelude::*;

fn get_hostname() -> Resolved {
    Ok(hostname::get()
        .map_err(|error| format!("failed to get hostname: {}", error))?
        .to_string_lossy()
        .into())
}

#[derive(Clone, Copy, Debug)]
pub struct GetHostname;

impl Function for GetHostname {
    fn identifier(&self) -> &'static str {
        "get_hostname"
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        _: ArgumentList,
    ) -> Compiled {
        Ok(Box::new(GetHostnameFn))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: r#"get_hostname!() != """#,
            result: Ok("true"),
        }]
    }

    fn call_by_vm(&self, _ctx: &mut Context, _args: &mut VmArgumentList) -> Resolved {
        get_hostname()
    }
}

#[derive(Debug, Clone)]
struct GetHostnameFn;

impl Expression for GetHostnameFn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        get_hostname()
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}
