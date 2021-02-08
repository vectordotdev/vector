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
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let condition = arguments.required("condition")?.boxed();
        let message = arguments.optional("message").map(Expr::boxed);

        Ok(Box::new(AssertFn { condition, message }))
    }
}

#[derive(Debug, Clone)]
struct AssertFn {
    condition: Box<dyn Expression>,
    message: Option<Box<dyn Expression>>,
}

impl AssertFn {
    #[cfg(test)]
    fn new(condition: Box<dyn Expression>, message: Option<Box<dyn Expression>>) -> Self {
        Self { condition, message }
    }
}

impl Expression for AssertFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let condition = self.condition.execute(state, object)?.try_boolean()?;
        if condition {
            Ok(Value::Null)
        } else {
            let message = match self.message.as_ref() {
                Some(message) => message
                    .execute(state, object)?
                    .try_bytes_utf8_lossy()?
                    .into_owned(),
                None => {
                    // If Expression implemented Display, we could could return the expression here.
                    "evaluated to false".to_string()
                }
            };
            Err(Error::Assert(message))
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.condition
            .type_def(state)
            .into_fallible(true)
            .with_constraint(value::Kind::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    #[test]
    fn assert() {
        let cases = vec![
            (
                btreemap! { "test" => false },
                Err("assertion failed: This has not gone well".to_string()),
                AssertFn::new(
                    Box::new(Path::from("test")),
                    Some(Box::new(Literal::from(Value::from(
                        "This has not gone well",
                    )))),
                ),
            ),
            (
                btreemap! { "test" => false },
                Err("assertion failed: evaluated to false".to_string()),
                AssertFn::new(Box::new(Path::from("test")), None),
            ),
            (
                btreemap! { "test" => true },
                Ok(Value::Null),
                AssertFn::new(Box::new(Path::from("test")), None),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object = Value::Map(object);
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
