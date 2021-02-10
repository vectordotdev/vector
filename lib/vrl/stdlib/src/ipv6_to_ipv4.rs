use std::net::IpAddr;

use vrl::prelude::*;

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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(Ipv6ToIpV4Fn { value }))
    }
}

#[derive(Debug, Clone)]
struct Ipv6ToIpV4Fn {
    value: Box<dyn Expression>,
}

impl Expression for Ipv6ToIpV4Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let ip = self
            .value
            .resolve(ctx)?
            .unwrap_bytes_utf8_lossy()
            .parse()
            .map_err(|err| format!("unable to parse IP address: {}", err))?;

        match ip {
            IpAddr::V4(addr) => Ok(addr.to_ipv6_mapped().to_string().into()),
            IpAddr::V6(addr) => match addr.to_ipv4() {
                Some(addr) => Ok(addr.to_string().into()),
                None => Err(format!("IPV6 address {} is not compatible with IPV4", addr).into()),
            },
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![value_string {
//         expr: |_| Ipv6ToIpV4Fn {
//             value: Literal::from("192.168.0.1").boxed()
//         },
//         def: TypeDef {
//             kind: value::Kind::Bytes,
//             fallible: true,
//             ..Default::default()
//         },
//     }];

//     #[test]
//     fn ipv6_to_ipv4() {
//         let cases = vec![
//             (
//                 map!["foo": "i am not an ipaddress"],
//                 Err("function call error: unable to parse IP address: invalid IP address syntax".to_string()),
//                 Ipv6ToIpV4Fn::new(Box::new(Path::from("foo"))),
//             ),
//             (
//                 map!["foo": "2001:0db8:85a3::8a2e:0370:7334"],
//                 Err("function call error: IPV6 address 2001:db8:85a3::8a2e:370:7334 is not compatible with IPV4".to_string()),
//                 Ipv6ToIpV4Fn::new(Box::new(Path::from("foo"))),
//             ),
//             (
//                 map!["foo": "::ffff:192.168.0.1"],
//                 Ok(Value::from("192.168.0.1")),
//                 Ipv6ToIpV4Fn::new(Box::new(Path::from("foo"))),
//             ),
//             (
//                 map!["foo": "0:0:0:0:0:ffff:c633:6410"],
//                 Ok(Value::from("198.51.100.16")),
//                 Ipv6ToIpV4Fn::new(Box::new(Path::from("foo"))),
//             ),
//         ];

//         let mut state = state::Program::default();

//         for (object, exp, func) in cases {
//             let mut object = Value::Map(object);
//             let got = func
//                 .resolve(&mut ctx)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }
// }
