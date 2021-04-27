use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct GetHostIp;

impl Function for GetHostIp {
    fn identifier(&self) -> &'static str {
        "get_host_ip"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "interface",
                kind: kind::BYTES,
                required: false,
            },
            Parameter {
                keyword: "family",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let families = vec![value!("IPv4"), value!("IPv6")];
        let interface = arguments.optional("interface");
        let family = arguments
            .optional_enum("family", &families)?
            .map(|v| v.try_bytes().expect("family not bytes"));
        Ok(Box::new(GetHostIpFn { interface, family }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "valid",
                source: r#"get_host_ip!() != null"#,
                result: Ok("true"),
            },
            Example {
                title: "IPv4",
                source: r#"get_host_ip!(family: "IPv4") != null"#,
                result: Ok("true"),
            },
            Example {
                title: "interface",
                source: r#"get_host_ip!(interface: "eth0") != null"#,
                result: Ok("true"),
            },
        ]
    }
}

#[derive(Debug, Clone)]
struct GetHostIpFn {
    interface: Option<Box<dyn Expression>>,
    family: Option<Bytes>,
}

impl Expression for GetHostIpFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let interface = self
            .interface
            .as_ref()
            .map(|d| d.resolve(ctx))
            .transpose()?
            .map(|d| d.try_bytes())
            .transpose()?;

        // This attempts to just find the first non-loopback
        // interface that is up and grab its first IP
        pnet_datalink::interfaces()
            .iter()
            // If user specified a interface, find that one, otherwise find the first non-loopback
            // interface that is up
            .find(|e| match &interface {
                Some(interface) => e.name.as_bytes() == interface,
                None => e.is_up() && !e.is_loopback() && !e.ips.is_empty(),
            })
            // If the user specifid an address family, find the first address matching that family,
            // otherwise find the first ip
            .and_then(|interface| match self.family.as_ref().map(|b| b.as_ref()) {
                Some(b"IPv4") => interface.ips.iter().find(|ip| ip.is_ipv4()),
                Some(b"IPv6") => interface.ips.iter().find(|ip| ip.is_ipv6()),
                None => interface.ips.get(0),
                _ => unreachable!("enum invariant"),
            })
            .map(|ip| ip.ip().to_string())
            .map(Into::into)
            .ok_or_else(|| "unable to find IP address".into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[cfg(target_os = "linux")]
    test_function![
        get_host_ip => GetHostIp;

        loopback {
            args: func_args![interface: "lo", family: "IPv4"],
            want: Ok("127.0.0.1"),
            tdef: TypeDef::new().fallible().bytes(),
        }
    ];

    #[cfg(target_os = "macos")]
    test_function![
        get_host_ip => GetHostIp;

        loopback {
            args: func_args![interface: "lo0", family: "IPv4"],
            want: Ok("127.0.0.1"),
            tdef: TypeDef::new().fallible().bytes(),
        }
    ];

    #[test]
    fn get_host_ip_first() {
        let mut state = vrl::state::Runtime::default();
        let mut object: Value = map![].into();
        let mut ctx = Context::new(&mut object, &mut state);
        let value = GetHostIpFn {
            interface: None,
            family: None,
        }
        .resolve(&mut ctx)
        .unwrap();

        assert!(matches!(&value, Value::Bytes(_)));

        match value {
            Value::Bytes(val) => {
                let val = String::from_utf8_lossy(&val);
                val.parse::<IpAddr>().expect("valid ip address");
            }
            _ => unreachable!(),
        }
    }
}
