use std::net::IpAddr;

use ::value::Value;
use bytes::Bytes;
use vrl::prelude::*;

fn ip_pton(value: Value) -> Resolved {
    let ip: IpAddr = value
        .try_bytes_utf8_lossy()?
        .parse()
        .map_err(|err| format!("unable to parse IP address: {err}"))?;

    let bytes = match ip {
        IpAddr::V4(ipv4) => Bytes::copy_from_slice(&ipv4.octets()),
        IpAddr::V6(ipv6) => Bytes::copy_from_slice(&ipv6.octets()),
    };

    Ok(bytes.into())
}

#[derive(Clone, Copy, Debug)]
pub struct IpPton;

impl Function for IpPton {
    fn identifier(&self) -> &'static str {
        "ip_pton"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "Convert IPv4 address to bytes and encode to Base64",
                source: r#"encode_base64(ip_pton!("192.168.0.1"))"#,
                result: Ok("wKgAAQ=="),
            },
            Example {
                title: "Convert IPv6 address to bytes and encode to Base64",
                source: r#"encode_base64(ip_pton!("2001:db8:85a3::8a2e:370:7334"))"#,
                result: Ok("IAENuIWjAAAAAIouA3BzNA=="),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(IpPtonFn { value }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct IpPtonFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for IpPtonFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        ip_pton(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        ip_pton => IpPton;

        invalid {
            args: func_args![value: "i am not an ipaddress"],
            want: Err("unable to parse IP address: invalid IP address syntax"),
            tdef: TypeDef::bytes().fallible(),
        }

        valid_ipv4 {
            args: func_args![value: "1.2.3.4"],
            want: Ok(value!("\x01\x02\x03\x04")),
            tdef: TypeDef::bytes().fallible(),
        }

        valid_ipv6 {
            args: func_args![value: "102:304:506:708:90a:b0c:d0e:f10"],
            want: Ok(value!("\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10")),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
