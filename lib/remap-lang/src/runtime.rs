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
            .map(|expression| expression.execute(&mut self.state, object))
            .collect::<crate::Result<Vec<Value>>>()
            .map_err(|err| RuntimeError::from((program.source, err)))?;

        Ok(values.pop().unwrap_or(Value::Null))
    }
}
