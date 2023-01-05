use ::value::Value;
use cidr_utils::cidr::IpCidr;
use vrl::prelude::*;

fn ip_cidr_contains(value: Value, cidr: Value) -> Resolved {
    let value = value
        .try_bytes_utf8_lossy()?
        .parse()
        .map_err(|err| format!("unable to parse IP address: {err}"))?;
    let cidr = {
        let cidr = cidr.try_bytes_utf8_lossy()?;

        IpCidr::from_str(cidr).map_err(|err| format!("unable to parse CIDR: {err}"))?
    };
    Ok(cidr.contains(value).into())
}

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

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let cidr = arguments.required("cidr");
        let value = arguments.required("value");

        Ok(IpCidrContainsFn { cidr, value }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct IpCidrContainsFn {
    cidr: Box<dyn Expression>,
    value: Box<dyn Expression>,
}

impl FunctionExpression for IpCidrContainsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let cidr = self.cidr.resolve(ctx)?;

        ip_cidr_contains(value, cidr)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::boolean().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function! [
        ip_cidr_contains => IpCidrContains;

        ipv4_yes {
            args: func_args![value: "192.168.10.32",
                             cidr: "192.168.0.0/16",
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().fallible(),
        }

        ipv4_no {
            args: func_args![value: "192.168.10.32",
                             cidr: "192.168.0.0/24",
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().fallible(),
        }

        ipv6_yes {
            args: func_args![value: "2001:4f8:3:ba:2e0:81ff:fe22:d1f1",
                             cidr: "2001:4f8:3:ba::/64",
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().fallible(),
        }

        ipv6_no {
            args: func_args![value: "2001:4f8:3:ba:2e0:81ff:fe22:d1f1",
                             cidr: "2001:4f8:4:ba::/64",
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().fallible(),
        }
    ];
}
