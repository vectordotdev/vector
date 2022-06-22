use std::{cell::RefCell, error::Error, fmt, rc::Rc};

use compiler::ExpressionError;
use lookup::LookupBuf;
use value::Value;
use vector_common::TimeZone;

use crate::{state, BatchContext, Context, Program, Target};

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
    ) -> RuntimeResult {
        // Validate that the path is a value.
        match target.target_get(&self.root_lookup) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Err(Terminate::Error(
                    "expected target object, got nothing".to_owned().into(),
                ))
            }
            Err(err) => {
                return Err(Terminate::Error(
                    format!("error querying target object: {}", err).into(),
                ))
            }
        };

        let mut ctx = Context::new(target, &mut self.state, timezone);

        program.resolve(&mut ctx).map_err(|err| match err {
            #[cfg(feature = "expr-abort")]
            ExpressionError::Abort { .. } => Terminate::Abort(err),
            err @ ExpressionError::Error { .. } => Terminate::Error(err),
        })
    }
}

#[derive(Debug, Default)]
pub struct BatchRuntime {
    root_lookup: LookupBuf,
}

impl BatchRuntime {
    pub fn new() -> Self {
        Self {
            // `LookupBuf` uses a `VecDeque` internally, which always allocates, even
            // when it's empty (for `LookupBuf::root()`), so we do the
            // allocation on initialization of the runtime, instead of on every
            // `resolve` run.
            root_lookup: LookupBuf::root(),
        }
    }

    /// Given the provided [`Target`], resolve the provided [`Program`] to
    /// completion.
    pub fn resolve_batch<'a>(
        &self,
        targets: Vec<Rc<RefCell<dyn Target + 'a>>>,
        program: &Program,
        timezone: TimeZone,
    ) -> Vec<(Rc<RefCell<dyn Target + 'a>>, RuntimeResult)> {
        let mut invalid_targets = Vec::new();
        let (indices, targets) = (0..targets.len())
            .into_iter()
            .zip(targets)
            .filter_map(|(index, target)| {
                // Validate that the path is a value.
                match target.clone().borrow().target_get(&self.root_lookup) {
                    Ok(Some(_)) => Some((index, target)),
                    Ok(None) => {
                        invalid_targets.push((
                            index,
                            target,
                            Err(Terminate::Error(
                                "expected target object, got nothing".to_owned().into(),
                            )),
                        ));
                        None
                    }
                    Err(err) => {
                        invalid_targets.push((
                            index,
                            target,
                            Err(Terminate::Error(
                                format!("error querying target object: {}", err).into(),
                            )),
                        ));
                        None
                    }
                }
            })
            .unzip::<_, _, Vec<usize>, Vec<Rc<RefCell<dyn Target + 'a>>>>();

        let values = vec![Ok(Value::Null); indices.len()];
        let states = (0..indices.len())
            .map(|_| Rc::new(RefCell::new(state::Runtime::default())))
            .collect::<Vec<_>>();
        let mut ctx = BatchContext::new(indices, values, targets, states, timezone);
        program.resolve_batch(&mut ctx);

        let (indices, resolved_values, targets, _, _) = ctx.into_parts();
        let resolved_values = resolved_values.into_iter().map(|resolved| {
            resolved.map_err(|err| match err {
                #[cfg(feature = "expr-abort")]
                ExpressionError::Abort { .. } => Terminate::Abort(err),
                err @ ExpressionError::Error { .. } => Terminate::Error(err),
            })
        });

        let mut result = indices
            .into_iter()
            .zip(targets)
            .zip(resolved_values)
            .map(|((index, target), resolved)| (index, target, resolved))
            .chain(invalid_targets)
            .collect::<Vec<_>>();

        result.sort_unstable_by(|(a, ..), (b, ..)| b.cmp(a));

        result
            .into_iter()
            .map(|(_, target, resolved)| (target, resolved))
            .collect()
    }
}
