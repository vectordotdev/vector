use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Assert;

impl Function for Assert {
    fn identifier(&self) -> &'static str {
        "assert"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "condition",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: true,
            },
            Parameter {
                keyword: "message",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let condition = arguments.required_expr("condition")?;
        let message = arguments.required_expr("message")?;

        Ok(Box::new(AssertFn { condition, message }))
    }
}

#[derive(Debug, Clone)]
struct AssertFn {
    condition: Box<dyn Expression>,
    message: Box<dyn Expression>,
}

impl AssertFn {
    #[cfg(test)]
    fn new(condition: Box<dyn Expression>, message: Box<dyn Expression>) -> Self {
        Self { condition, message }
    }
}

impl Expression for AssertFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let condition = self.condition.execute(state, object)?.try_boolean()?;
        let bytes = self.message.execute(state, object)?.try_bytes()?;
        let message = String::from_utf8_lossy(&bytes).into_owned();

        if condition {
            Ok(Value::Null)
        } else {
            Err(Error::Assert(message))
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.condition
            .type_def(state)
            .fallible_unless(value::Kind::Boolean)
            .merge(
                self.message
                    .type_def(state)
                    .fallible_unless(value::Kind::Bytes),
            )
            .with_constraint(value::Kind::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn assert() {
        let cases = vec![(
            map!["test": false],
            Err("assertion failed: This has not gone well".to_string()),
            AssertFn::new(
                Box::new(Path::from("test")),
                Box::new(Literal::from(Value::from("This has not gone well"))),
            ),
        )];

        let mut state = state::Program::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
