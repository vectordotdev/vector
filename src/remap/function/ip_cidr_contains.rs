use cidr_utils::cidr::IpCidr;
use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IpCidrContains;

impl Function for IpCidrContains {
    fn identifier(&self) -> &'static str {
        "ip_cidr_contains"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "cidr",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let cidr = arguments.required_expr("cidr")?;

        Ok(Box::new(IpCidrContainsFn { value, cidr }))
    }
}

#[derive(Debug, Clone)]
struct IpCidrContainsFn {
    value: Box<dyn Expression>,
    cidr: Box<dyn Expression>,
}

impl IpCidrContainsFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, cidr: Box<dyn Expression>) -> Self {
        Self { value, cidr }
    }
}

impl Expression for IpCidrContainsFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = {
            let bytes = self.value.execute(state, object)?.try_bytes()?;
            String::from_utf8_lossy(&bytes)
                .parse()
                .map_err(|err| format!("unable to parse IP address: {}", err))?
        };

        let cidr = {
            let bytes = self.cidr.execute(state, object)?.try_bytes()?;
            let cidr = String::from_utf8_lossy(&bytes);
            IpCidr::from_str(cidr).map_err(|err| format!("unable to parse CIDR: {}", err))?
        };

        Ok(Value::from(cidr.contains(value)))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .merge(
                self.cidr
                    .type_def(state)
                    .fallible_unless(value::Kind::Bytes),
            )
            .with_constraint(value::Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    remap::test_type_def![value_string {
        expr: |_| IpCidrContainsFn {
            value: Literal::from("192.168.0.1").boxed(),
            cidr: Literal::from("192.168.0.0/16").boxed()
        },
        def: TypeDef {
            kind: value::Kind::Boolean,
            ..Default::default()
        },
    }];

    #[test]
    fn ip_cidr_contains() {
        let cases = vec![
            (
                map!["foo": "192.168.10.32",
                     "cidr": "192.168.0.0/16",
                ],
                Ok(Value::from(true)),
                IpCidrContainsFn::new(Box::new(Path::from("foo")), Box::new(Path::from("cidr"))),
            ),
            (
                map!["foo": "192.168.10.32",
                     "cidr": "192.168.0.0/24",
                ],
                Ok(Value::from(false)),
                IpCidrContainsFn::new(Box::new(Path::from("foo")), Box::new(Path::from("cidr"))),
            ),
        ];

        let mut state = state::Program::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
