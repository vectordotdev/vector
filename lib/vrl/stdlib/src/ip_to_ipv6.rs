use std::net::IpAddr;

use vrl::prelude::*;

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
        _state: &state::Compiler,
        _info: &FunctionCompileInfo,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IpToIpv6Fn { value }))
    }
}

#[derive(Debug, Clone)]
struct IpToIpv6Fn {
    value: Box<dyn Expression>,
}

impl Expression for IpToIpv6Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let ip: IpAddr = self
            .value
            .resolve(ctx)?
            .try_bytes_utf8_lossy()?
            .parse()
            .map_err(|err| format!("unable to parse IP address: {}", err))?;

        match ip {
            IpAddr::V4(addr) => Ok(addr.to_ipv6_mapped().to_string().into()),
            IpAddr::V6(addr) => Ok(addr.to_string().into()),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
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
            tdef: TypeDef::new().fallible().bytes(),
        }

        valid {
            args: func_args![value: "192.168.0.1"],
            want: Ok(value!("::ffff:192.168.0.1")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        ipv6_passthrough {
            args: func_args![value: "2404:6800:4003:c02::64"],
            want: Ok(value!("2404:6800:4003:c02::64")),
            tdef: TypeDef::new().fallible().bytes(),
        }
    ];
}
