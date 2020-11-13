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
                accepts: |v| matches!(v, Value::String(_)),
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
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let condition = required!(state, object, self.condition, Value::Boolean(v) => v);
        let message = {
            let bytes = required!(state, object, self.message, Value::String(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        if condition {
            Ok(None)
        } else {
            Err(Error::Assert(message))
        }
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

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
