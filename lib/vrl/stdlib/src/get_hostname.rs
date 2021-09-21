use vrl::prelude::{Function};

#[cfg(not(feature = "wasm_compatible"))]
use hostname;

#[derive(Clone, Copy, Debug)]
pub struct GetHostname;

impl Function for GetHostname {
    fn identifier(&self) -> &'static str {
        "get_hostname"
    }

    fn compile(&self, _state: &state::Compiler, _: ArgumentList) -> Compiled {
        Ok(Box::new(GetHostnameFn))
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

impl Expression for GetHostnameFn {
    #[cfg(feature = "wasm_compatible")]
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok("vrl_web_host".into())
    }

    #[cfg(not(feature = "wasm_compatible"))]
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(hostname::get()
            .map_err(|error| format!("failed to get hostname: {}", error))?
            .to_string_lossy()
            .into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}
