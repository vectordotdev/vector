use std::net::IpAddr;

use dns_lookup::lookup_addr;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ReverseDns;

impl Function for ReverseDns {
    fn identifier(&self) -> &'static str {
        "reverse_dns"
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
            title: "Example",
            source: r#"reverse_dns!("127.0.0.1")"#,
            result: Ok("localhost"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _info: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ReverseDnsFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ReverseDnsFn {
    value: Box<dyn Expression>,
}

impl Expression for ReverseDnsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let ip: IpAddr = self
            .value
            .resolve(ctx)?
            .try_bytes_utf8_lossy()?
            .parse()
            .map_err(|err| format!("unable to parse IP address: {}", err))?;

        let host =
            lookup_addr(&ip).map_err(|err| format!("unable to perform a lookup : {}", err))?;

        Ok(host.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value.type_def(state).fallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        reverse_dns => ReverseDns;

        invalid_ip {
            args: func_args![value: value!("999.999.999.999")],
            want: Err("unable to parse IP address: invalid IP address syntax"),
            tdef: TypeDef::new().fallible().bytes(),
        }

        google_ipv4 {
            args: func_args![value: value!("8.8.8.8")],
            want: Ok(value!("dns.google")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        google_ipv6 {
            args: func_args![value: value!("2001:4860:4860::8844")],
            want: Ok(value!("dns.google")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        invalid_type {
            args: func_args![value: value!(1)],
            want: Err("expected \"string\", got \"integer\""),
            tdef: TypeDef::new().fallible().bytes(),
        }
    ];
}
