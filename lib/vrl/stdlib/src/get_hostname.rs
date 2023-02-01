use vrl::prelude::*;

fn get_hostname() -> Resolved {
    Ok(hostname::get()
        .map_err(|error| format!("failed to get hostname: {error}"))?
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
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        _: ArgumentList,
    ) -> Compiled {
        Ok(GetHostnameFn.as_expr())
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: r#"get_hostname!() != """#,
            result: Ok("true"),
        }]
    }
}

#[derive(Debug, Clone)]
struct GetHostnameFn;

impl FunctionExpression for GetHostnameFn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        get_hostname()
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}
