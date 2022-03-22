use std::net::IpAddr;

use vrl::prelude::*;

fn ip_ntop(value: Value) -> Resolved {
    let value = value.try_bytes()?;

    match value.len() {
        4 => {
            let bytes: [u8; 4] = value[..].try_into().expect("invalid length");
            Ok(IpAddr::from(bytes).to_string().into())
        }
        16 => {
            let bytes: [u8; 16] = value[..].try_into().expect("invalid length");
            Ok(IpAddr::from(bytes).to_string().into())
        }
        _ => Err(r#""value" must be of length 4 or 16 bytes"#.into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IpNtop;

impl Function for IpNtop {
    fn identifier(&self) -> &'static str {
        "ip_ntop"
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
                title: "Convert IPv4 address from bytes after decoding from Base64",
                source: r#"ip_ntop!(decode_base64!("wKgAAQ=="))"#,
                result: Ok("192.168.0.1"),
            },
            Example {
                title: "Convert IPv6 address from bytes after decoding from Base64",
                source: r#"ip_ntop!(decode_base64!("IAENuIWjAAAAAIouA3BzNA=="))"#,
                result: Ok("2001:db8:85a3::8a2e:370:7334"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IpNtopFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        ip_ntop(value)
    }
}

#[derive(Debug, Clone)]
struct IpNtopFn {
    value: Box<dyn Expression>,
}

impl Expression for IpNtopFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        ip_ntop(value)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        ip_ntop => IpNtop;

        invalid {
            args: func_args![value: "\x01\x02"],
            want: Err(r#""value" must be of length 4 or 16 bytes"#),
            tdef: TypeDef::bytes().fallible(),
        }

        valid_ipv4 {
            args: func_args![value: "\x01\x02\x03\x04"],
            want: Ok(value!("1.2.3.4")),
            tdef: TypeDef::bytes().fallible(),
        }

        valid_ipv6 {
            args: func_args![value: "\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10"],
            want: Ok(value!("102:304:506:708:90a:b0c:d0e:f10")),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
