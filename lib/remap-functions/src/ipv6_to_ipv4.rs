use std::net::IpAddr;

use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Ipv6ToIpV4;

impl Function for Ipv6ToIpV4 {
    fn identifier(&self) -> &'static str {
        "ipv6_to_ipv4"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(Ipv6ToIpV4Fn { value }))
    }
}

#[derive(Debug, Clone)]
struct Ipv6ToIpV4Fn {
    value: Box<dyn Expression>,
}

impl Expression for Ipv6ToIpV4Fn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let ip = {
            let bytes = self.value.execute(state, object)?.try_bytes()?;
            String::from_utf8_lossy(&bytes)
                .parse()
                .map_err(|err| format!("unable to parse IP address: {}", err))?
        };

        match ip {
            IpAddr::V4(addr) => Ok(addr.to_string().into()),
            IpAddr::V6(addr) => match addr.to_ipv4() {
                Some(addr) => Ok(addr.to_string().into()),
                None => Err(format!("IPV6 address {} is not compatible with IPV4", addr).into()),
            },
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .into_fallible(true)
            .with_constraint(value::Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    remap::test_type_def![value_string {
        expr: |_| Ipv6ToIpV4Fn {
            value: Literal::from("192.168.0.1").boxed()
        },
        def: TypeDef {
            kind: value::Kind::Bytes,
            fallible: true,
            ..Default::default()
        },
    }];

    test_function![
        ipv6_to_ipv4 => Ipv6ToIpV4;

        error {
            args: func_args![value: "i am not an ipaddress"],
            want: Err("function call error: unable to parse IP address: invalid IP address syntax".to_string()),
        }

        incompatible {
            args: func_args![value: "2001:0db8:85a3::8a2e:0370:7334"],
            want: Err("function call error: IPV6 address 2001:db8:85a3::8a2e:370:7334 is not compatible with IPV4".to_string()),
        }

        ipv4_compatible {
            args: func_args![value: "::ffff:192.168.0.1"],
            want: Ok(Value::from("192.168.0.1")),
        }

        ipv6 {
            args: func_args![value: "0:0:0:0:0:ffff:c633:6410"],
            want: Ok(Value::from("198.51.100.16")),
        }

        ipv4 {
            args: func_args![value: "198.51.100.16"],
            want: Ok(Value::from("198.51.100.16")),
        }
    ];
}
