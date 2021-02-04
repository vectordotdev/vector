use crate::{state, Context, Path, Program, Target, Value};
use std::{error::Error, fmt};

pub type RuntimeResult = Result<Value, Abort>;

#[derive(Debug, Default)]
pub struct Runtime {
    state: state::Runtime,
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
    pub fn new(state: state::Runtime) -> Self {
        Self { state }
    }

    /// Given the provided [`Target`], resolve the provided [`Program`] to
    /// completion.
    pub fn resolve(&mut self, target: &mut dyn Target, program: &Program) -> RuntimeResult {
        // Validate that the path is an object.
        //
        // VRL technically supports any `Value` object as the root, but the
        // assumption is people are expected to use it to query objects.
        match target.get(&Path::root()) {
            Ok(Some(Value::Object(_))) => {}
            Ok(Some(value)) => {
                return Err(Abort(format!(
                    "target must be a valid object, got {}: {}",
                    value.kind(),
                    value
                )))
            }
            Ok(None) => return Err(Abort("expected target object, got nothing".to_owned())),
            Err(err) => return Err(Abort(format!("error querying target object: {}", err))),
        };

        let mut context = Context::new(target, &mut self.state);

        let mut values = program
            .iter()
            .map(|expr| {
                expr.resolve(&mut context)
                    .map_err(|err| Abort(err.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(values.pop().unwrap_or(Value::Null))
    }
}
