use std::net::IpAddr;

use ::value::Value;
use vrl::prelude::*;

fn ip_to_ipv6(value: Value) -> Resolved {
    let ip: IpAddr = value
        .try_bytes_utf8_lossy()?
        .parse()
        .map_err(|err| format!("unable to parse IP address: {err}"))?;
    match ip {
        IpAddr::V4(addr) => Ok(addr.to_ipv6_mapped().to_string().into()),
        IpAddr::V6(addr) => Ok(addr.to_string().into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IpToIpv6;

impl Function for IpToIpv6 {
    fn identifier(&self) -> &'static str {
        "ip_to_ipv6"
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
            title: "valid IPv4",
            source: r#"ip_to_ipv6!("192.168.0.1")"#,
            result: Ok("::ffff:192.168.0.1"),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(IpToIpv6Fn { value }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct IpToIpv6Fn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for IpToIpv6Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        ip_to_ipv6(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        ip_to_ipv6 => IpToIpv6;

        invalid {
            args: func_args![value: "i am not an ipaddress"],
            want: Err(
                    "unable to parse IP address: invalid IP address syntax"),
            tdef: TypeDef::bytes().fallible(),
        }

        valid {
            args: func_args![value: "192.168.0.1"],
            want: Ok(value!("::ffff:192.168.0.1")),
            tdef: TypeDef::bytes().fallible(),
        }

        ipv6_passthrough {
            args: func_args![value: "2404:6800:4003:c02::64"],
            want: Ok(value!("2404:6800:4003:c02::64")),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
