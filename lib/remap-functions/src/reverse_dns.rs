use dns_lookup::lookup_addr;
use std::net::IpAddr;

use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ReverseDns;

impl Function for ReverseDns {
    fn identifier(&self) -> &'static str {
        "reverse_dns"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "ip",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let ip = arguments.required("ip")?.boxed();

        Ok(Box::new(ReverseDnsFn { ip }))
    }
}

#[derive(Debug, Clone)]
struct ReverseDnsFn {
    ip: Box<dyn Expression>,
}

impl Expression for ReverseDnsFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.ip.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);
        let ip: IpAddr = value
            .parse()
            .map_err(|err| format!("unable to parse IP address: {}", err))?;
        let host =
            lookup_addr(&ip).map_err(|err| format!("unable to perform a lookup : {}", err))?;

        Ok(host.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.ip
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    remap::test_type_def![value_string {
        expr: |_| ReverseDnsFn {
            ip: Literal::from("192.168.0.1").boxed()
        },
        def: TypeDef {
            kind: value::Kind::Bytes,
            ..Default::default()
        },
    }];

    test_function![
        reverse_dns => ReverseDns;

        invalid_ip {
            args: func_args![ip: value!("999.999.999.999")],
            want: Err("function call error: unable to parse IP address: invalid IP address syntax".to_string())
        }

        localhost {
            args: func_args![ip: value!("127.0.0.1")],
            want: Ok(value!("localhost"))
        }
    ];
}
