use crate::error::RuntimeError;
use crate::{state, Expression, Object, Program, Value};

#[derive(Debug, Default)]
pub struct Runtime {
    state: state::Program,
}

impl Runtime {
    pub fn new(state: state::Program) -> Self {
        Self { state }
    }

    /// Given the provided [`Object`], run the provided [`Program`] to
    /// completion.
    pub fn run<'a>(
        &mut self,
        object: &mut impl Object,
        program: &'a Program,
    ) -> Result<Value, RuntimeError<'a>> {
        let mut values = program
            .expressions
            .iter()
            .map(|expr| {
                expr.execute(&mut self.state, object)
                    .map_err(|err| (expr, err))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|(expr, err)| RuntimeError::from((program.source, expr, err)))?;

        Ok(values.pop().unwrap_or(Value::Null))
    }
}
