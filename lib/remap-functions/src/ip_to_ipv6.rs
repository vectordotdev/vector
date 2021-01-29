use std::net::IpAddr;

use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IpToIpv6;

impl Function for IpToIpv6 {
    fn identifier(&self) -> &'static str {
        "ip_to_ipv6"
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

        Ok(Box::new(IpToIpv6Fn { value }))
    }
}

#[derive(Debug, Clone)]
struct IpToIpv6Fn {
    value: Box<dyn Expression>,
}

impl IpToIpv6Fn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for IpToIpv6Fn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let ip: IpAddr = self
            .value
            .execute(state, object)?
            .try_bytes_utf8_lossy()?
            .parse()
            .map_err(|err| format!("unable to parse IP address: {}", err))?;

        match ip {
            IpAddr::V4(addr) => Ok(addr.to_ipv6_mapped().to_string().into()),
            IpAddr::V6(addr) => Ok(addr.to_string().into()),
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
    use shared::btreemap;

    remap::test_type_def![value_string {
        expr: |_| IpToIpv6Fn {
            value: Literal::from("192.168.0.1").boxed()
        },
        def: TypeDef {
            kind: value::Kind::Bytes,
            fallible: true,
            ..Default::default()
        },
    }];

    #[test]
    fn ip_to_ipv6() {
        let cases = vec![
            (
                btreemap! { "foo" => "i am not an ipaddress" },
                Err(
                    "function call error: unable to parse IP address: invalid IP address syntax"
                        .to_string(),
                ),
                IpToIpv6Fn::new(Box::new(Path::from("foo"))),
            ),
            (
                btreemap! { "foo" => "192.168.0.1" },
                Ok(Value::from("::ffff:192.168.0.1")),
                IpToIpv6Fn::new(Box::new(Path::from("foo"))),
            ),
            (
                btreemap! { "foo" => "2404:6800:4003:c02::64" },
                Ok(Value::from("2404:6800:4003:c02::64")),
                IpToIpv6Fn::new(Box::new(Path::from("foo"))),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object = Value::Map(object);
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
