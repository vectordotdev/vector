use std::net::IpAddr;

use ::value::Value;
use dns_lookup::lookup_addr;
use vrl::prelude::*;

fn reverse_dns(value: Value) -> Result<Value> {
    let ip: IpAddr = value
        .try_bytes_utf8_lossy()?
        .parse()
        .map_err(|err| format!("unable to parse IP address: {}", err))?;
    let host = lookup_addr(&ip).map_err(|err| format!("unable to perform a lookup : {}", err))?;

    Ok(host.into())
}

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
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ReverseDnsFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        reverse_dns(value)
    }
}

#[derive(Debug, Clone)]
struct ReverseDnsFn {
    value: Box<dyn Expression>,
}

impl Expression for ReverseDnsFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?.into_owned();
        reverse_dns(value).map(Cow::Owned)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().fallible()
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
            tdef: TypeDef::bytes().fallible(),
        }

        google_ipv4 {
            args: func_args![value: value!("8.8.8.8")],
            want: Ok(value!("dns.google")),
            tdef: TypeDef::bytes().fallible(),
        }

        google_ipv6 {
            args: func_args![value: value!("2001:4860:4860::8844")],
            want: Ok(value!("dns.google")),
            tdef: TypeDef::bytes().fallible(),
        }

        invalid_type {
            args: func_args![value: value!(1)],
            want: Err("expected string, got integer"),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
