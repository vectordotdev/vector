use std::net::IpAddr;

use ::value::Value;
use vrl::prelude::*;

fn ipv6_to_ipv4(value: Value) -> Resolved {
    let ip = value
        .try_bytes_utf8_lossy()?
        .parse()
        .map_err(|err| format!("unable to parse IP address: {err}"))?;
    match ip {
        IpAddr::V4(addr) => Ok(addr.to_string().into()),
        IpAddr::V6(addr) => match addr.to_ipv4() {
            Some(addr) => Ok(addr.to_string().into()),
            None => Err(format!("IPV6 address {addr} is not compatible with IPV4").into()),
        },
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ipv6ToIpV4;

impl Function for Ipv6ToIpV4 {
    fn identifier(&self) -> &'static str {
        "ipv6_to_ipv4"
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
            title: "valid IPv6",
            source: r#"ipv6_to_ipv4!("::ffff:192.168.0.1")"#,
            result: Ok("192.168.0.1"),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        Ok(Ipv6ToIpV4Fn { value }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct Ipv6ToIpV4Fn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for Ipv6ToIpV4Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        ipv6_to_ipv4(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        ipv6_to_ipv4 => Ipv6ToIpV4;

        error {
            args: func_args![value: "i am not an ipaddress"],
            want: Err(r#"unable to parse IP address: invalid IP address syntax"#.to_string()),
            tdef: TypeDef::bytes().fallible(),
        }

        incompatible {
            args: func_args![value: "2001:0db8:85a3::8a2e:0370:7334"],
            want: Err("IPV6 address 2001:db8:85a3::8a2e:370:7334 is not compatible with IPV4".to_string()),
            tdef: TypeDef::bytes().fallible(),
        }

        ipv4_compatible {
            args: func_args![value: "::ffff:192.168.0.1"],
            want: Ok(Value::from("192.168.0.1")),
            tdef: TypeDef::bytes().fallible(),
        }

        ipv6 {
            args: func_args![value: "0:0:0:0:0:ffff:c633:6410"],
            want: Ok(Value::from("198.51.100.16")),
            tdef: TypeDef::bytes().fallible(),
        }

        ipv4 {
            args: func_args![value: "198.51.100.16"],
            want: Ok(Value::from("198.51.100.16")),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
