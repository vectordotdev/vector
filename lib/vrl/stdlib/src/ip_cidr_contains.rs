use cidr_utils::cidr::IpCidr;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IpCidrContains;

impl Function for IpCidrContains {
    fn identifier(&self) -> &'static str {
        "ip_cidr_contains"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "cidr",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "in range",
                source: r#"ip_cidr_contains!("192.168.0.0/16", "192.168.0.1")"#,
                result: Ok("true"),
            },
            Example {
                title: "not in range",
                source: r#"ip_cidr_contains!("192.168.0.0/24", "192.168.10.32")"#,
                result: Ok("false"),
            },
            Example {
                title: "invalid cidr",
                source: r#"ip_cidr_contains!("INVALID", "192.168.10.32")"#,
                result: Err(
                    r#"function call error for "ip_cidr_contains" at (0:45): unable to parse CIDR: The CIDR string is incorrect."#,
                ),
            },
            Example {
                title: "invalid address",
                source: r#"ip_cidr_contains!("192.168.0.0/24", "INVALID")"#,
                result: Err(
                    r#"function call error for "ip_cidr_contains" at (0:46): unable to parse IP address: invalid IP address syntax"#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let cidr = arguments.required("cidr");
        let value = arguments.required("value");

        Ok(Box::new(IpCidrContainsFn { cidr, value }))
    }
}

#[derive(Debug, Clone)]
struct IpCidrContainsFn {
    cidr: Box<dyn Expression>,
    value: Box<dyn Expression>,
}

impl Expression for IpCidrContainsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = {
            let value = self.value.resolve(ctx)?;

            value
                .unwrap_bytes_utf8_lossy()
                .parse()
                .map_err(|err| format!("unable to parse IP address: {}", err))?
        };

        let cidr = {
            let value = self.cidr.resolve(ctx)?;
            let cidr = value.unwrap_bytes_utf8_lossy();

            IpCidr::from_str(cidr).map_err(|err| format!("unable to parse CIDR: {}", err))?
        };

        Ok(cidr.contains(value).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().boolean()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![value_string {
//         expr: |_| IpCidrContainsFn {
//             value: Literal::from("192.168.0.1").boxed(),
//             cidr: Literal::from("192.168.0.0/16").boxed()
//         },
//         def: TypeDef {
//             kind: value::Kind::Boolean,
//             fallible: true,
//             ..Default::default()
//         },
//     }];

//     #[test]
//     fn ip_cidr_contains() {
//         let cases = vec![
//             (
//                 map!["foo": "192.168.10.32",
//                      "cidr": "192.168.0.0/16",
//                 ],
//                 Ok(Value::from(true)),
//                 IpCidrContainsFn::new(Box::new(Path::from("cidr")), Box::new(Path::from("foo"))),
//             ),
//             (
//                 map!["foo": "192.168.10.32",
//                      "cidr": "192.168.0.0/24",
//                 ],
//                 Ok(Value::from(false)),
//                 IpCidrContainsFn::new(Box::new(Path::from("cidr")), Box::new(Path::from("foo"))),
//             ),
//             (
//                 map!["foo": "2001:4f8:3:ba:2e0:81ff:fe22:d1f1",
//                      "cidr": "2001:4f8:3:ba::/64",
//                 ],
//                 Ok(Value::from(true)),
//                 IpCidrContainsFn::new(Box::new(Path::from("cidr")), Box::new(Path::from("foo"))),
//             ),
//             (
//                 map!["foo": "2001:4f8:3:ba:2e0:81ff:fe22:d1f1",
//                      "cidr": "2001:4f8:4:ba::/64",
//                 ],
//                 Ok(Value::from(false)),
//                 IpCidrContainsFn::new(Box::new(Path::from("cidr")), Box::new(Path::from("foo"))),
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
