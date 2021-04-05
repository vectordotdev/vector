use vrl::prelude::*;

use pnet::datalink;

#[derive(Clone, Copy, Debug)]
pub struct GetHostIp;

impl Function for GetHostIp {
    fn identifier(&self) -> &'static str {
        "get_host_ip"
    }

    fn compile(&self, _: ArgumentList) -> Compiled {
        Ok(Box::new(GetHostIpFn))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: r#"get_host_ip!() != null"#,
            result: Ok("true"),
        }]
    }
}

#[derive(Debug, Clone)]
struct GetHostIpFn;

impl Expression for GetHostIpFn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        // This attempts to just find the first non-loopback
        // interface that is up and grab its first IP
        let default_ip = datalink::interfaces()
            .iter()
            .find(|e| e.is_up() && !e.is_loopback() && !e.ips.is_empty())
            .and_then(|interface| interface.ips.get(0))
            .map(|ip| ip.ip().to_string());

        Ok(default_ip.map(Into::into).unwrap_or(Value::Null))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes().add_null()
    }
}
