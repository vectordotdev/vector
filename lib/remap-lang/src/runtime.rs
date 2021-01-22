use crate::{state, Expression, Object, Program, Value};
use std::{error::Error, fmt};

pub type RuntimeResult = Result<Value, Abort>;

#[derive(Debug, Default)]
pub struct Runtime {
    state: state::Program,
}

/// The error raised if the runtime is aborted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abort(String);

impl fmt::Display for Abort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for Abort {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl Runtime {
    pub fn new(state: state::Program) -> Self {
        Self { state }
    }

    /// Given the provided [`Object`], run the provided [`Program`] to
    /// completion.
    pub fn run<'a>(&mut self, object: &mut impl Object, program: &'a Program) -> RuntimeResult {
        let mut values = program
            .expressions
            .iter()
            .map(|expr| {
                expr.execute(&mut self.state, object)
                    .map_err(|err| Abort(err.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(values.pop().unwrap_or(Value::Null))
    }
}
