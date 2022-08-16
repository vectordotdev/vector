use std::net::Ipv6Addr;

use ::value::Value;
use vrl::prelude::*;
use vrl::state::TypeState;

fn is_ipv6(value: Value) -> Resolved {
    let value_str = value.try_bytes_utf8_lossy()?;
    Ok(value_str.parse::<Ipv6Addr>().is_ok().into())
}

#[derive(Clone, Copy, Debug)]
pub struct IsIpv6;

impl Function for IsIpv6 {
    fn identifier(&self) -> &'static str {
        "is_ipv6"
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
                title: "random string",
                source: r#"is_ipv6("foobar")"#,
                result: Ok("false"),
            },
            Example {
                title: "IPv4 address",
                source: r#"is_ipv6("1.1.1.1")"#,
                result: Ok("false"),
            },
            Example {
                title: "IPv6 address",
                source: r#"is_ipv6("2001:0db8:85a3:0000:0000:8a2e:0370:7334")"#,
                result: Ok("true"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(IsIpv6Fn { value }.as_expr())
    }
}

#[derive(Clone, Debug)]
struct IsIpv6Fn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for IsIpv6Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.value.resolve(ctx).and_then(is_ipv6)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_ipv6 => IsIpv6;

        not_string {
            args: func_args![value: value!(42)],
            want: Err("expected string, got integer"),
            tdef: TypeDef::boolean().infallible(),
        }

        random_string {
            args: func_args![value: value!("foobar")],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        ipv4_address {
            args: func_args![value: value!("1.1.1.1")],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        ipv6_address_valid {
            args: func_args![value: value!("2001:0db8:85a3:0000:0000:8a2e:0370:7334")],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        ipv6_address_invalid {
            args: func_args![value: value!("2001:0db8:85a3:zzzz:0000:8a2e:0370:7334")],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
