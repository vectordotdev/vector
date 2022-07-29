use std::{error::Error, fmt};

use compiler::{ExpressionError, Resolved};
use lookup::LookupBuf;
use value::Value;

use crate::{state, BatchContext, Context, Program, Target, TimeZone};

pub type RuntimeResult = Result<Value, Terminate>;

#[derive(Debug, Default)]
pub struct Runtime {
    state: state::Runtime,
    root_lookup: LookupBuf,
}

/// The error raised if the runtime is terminated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminate {
    /// A manual `abort` call.
    ///
    /// This is an intentional termination that does not result in an
    /// `Ok(Value)` result, but should neither be interpreted as an unexpected
    /// outcome.
    Abort(ExpressionError),

    /// An unexpected program termination.
    Error(ExpressionError),
}

impl fmt::Display for Terminate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Terminate::Abort(error) => error.fmt(f),
            Terminate::Error(error) => error.fmt(f),
        }
    }
}

impl Error for Terminate {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl From<ExpressionError> for Terminate {
    fn from(error: ExpressionError) -> Self {
        match error {
            #[cfg(feature = "expr-abort")]
            ExpressionError::Abort { .. } => Terminate::Abort(error),
            error @ ExpressionError::Error { .. } => Terminate::Error(error),
        }
    }
}

impl Runtime {
    pub fn new(state: state::Runtime) -> Self {
        Self {
            state,

            // `LookupBuf` uses a `VecDeque` internally, which always allocates, even
            // when it's empty (for `LookupBuf::root()`), so we do the
            // allocation on initialization of the runtime, instead of on every
            // `resolve` run.
            root_lookup: LookupBuf::root(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.state.is_empty()
    }

    pub fn clear(&mut self) {
        self.state.clear();
    }

    /// Given the provided [`Target`], resolve the provided [`Program`] to
    /// completion.
    pub fn resolve(
        &mut self,
        target: &mut dyn Target,
        program: &Program,
        timezone: &TimeZone,
    ) -> Resolved {
        // Validate that the path is a value.
        match target.target_get(&self.root_lookup) {
            Ok(Some(_)) => {}
            Ok(None) => return Err(ExpressionError::from("expected target object, got nothing")),
            Err(err) => {
                return Err(ExpressionError::from(format!(
                    "error querying target object: {}",
                    err
                )))
            }
        };

        let mut ctx = Context::new(target, &mut self.state, timezone);

        program.resolve(&mut ctx)
    }
}

#[derive(Debug, Default)]
pub struct BatchRuntime {
    root_lookup: LookupBuf,
    selection_vector: Vec<usize>,
}

impl BatchRuntime {
    pub fn new() -> Self {
        Self {
            // `LookupBuf` uses a `VecDeque` internally, which always allocates, even
            // when it's empty (for `LookupBuf::root()`), so we do the
            // allocation on initialization of the runtime, instead of on every
            // `resolve` run.
            root_lookup: LookupBuf::root(),
            selection_vector: vec![],
        }
    }

    /// Given the provided [`Target`], resolve the provided [`Program`] to
    /// completion.
    pub fn resolve_batch<'a>(
        &mut self,
        resolved_values: &'a mut Vec<Resolved>,
        targets: &'a mut [&'a mut dyn Target],
        states: &'a mut [state::Runtime],
        program: &mut Program,
        timezone: TimeZone,
    ) {
        self.selection_vector.resize(targets.len(), 0);
        for i in 0..self.selection_vector.len() {
            self.selection_vector[i] = i;
        }

        let mut len = self.selection_vector.len();
        let mut i = 0;
        loop {
            if i >= len {
                break;
            }

            let index = self.selection_vector[i];

            // Validate that the path is a value.
            match targets[index].target_get(&self.root_lookup) {
                Ok(Some(_)) => {
                    i += 1;
                }
                Ok(None) => {
                    resolved_values[index] =
                        Err(ExpressionError::from("expected target object, got nothing"));
                    len -= 1;
                    self.selection_vector.swap(i, len);
                }
                Err(err) => {
                    resolved_values[index] = Err(ExpressionError::from(format!(
                        "error querying target object: {}",
                        err
                    )));
                    len -= 1;
                    self.selection_vector.swap(i, len);
                }
            };
        }
        self.selection_vector.truncate(len);

        let mut ctx = BatchContext::new(resolved_values, targets, states, timezone);
        program.resolve_batch(&mut ctx, &self.selection_vector);
    }
}
