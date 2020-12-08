use crate::{state, Expression, Object, Program, RemapError, Value};

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
    pub fn execute(
        &mut self,
        object: &mut impl Object,
        program: &Program,
    ) -> Result<Value, RemapError> {
        let mut values = program
            .expressions
            .iter()
            .map(|expression| expression.execute(&mut self.state, object))
            .collect::<crate::Result<Vec<Value>>>()?;

        Ok(values.pop().unwrap_or(Value::Null))
    }
}
