use std::{cell::RefCell, error::Error, fmt, rc::Rc};

use compiler::ExpressionError;
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
    ) -> Vec<RuntimeResult> {
        let mut invalid_indices = Vec::new();
        let mut invalid_values = Vec::new();
        let (indices, targets) = (0..targets.len())
            .into_iter()
            .zip(targets)
            .filter_map(|(index, target)| {
                // Validate that the path is a value.
                match target.clone().borrow().target_get(&self.root_lookup) {
                    Ok(Some(_)) => Some((index, target)),
                    Ok(None) => {
                        invalid_indices.push(index);
                        invalid_values.push(Err(Terminate::Error(
                            "expected target object, got nothing".to_owned().into(),
                        )));
                        None
                    }
                    Err(err) => {
                        invalid_indices.push(index);
                        invalid_values.push(Err(Terminate::Error(
                            format!("error querying target object: {}", err).into(),
                        )));
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

        let (mut indices, resolved_values, _, _, _) = ctx.into_parts();
        let mut resolved_values = resolved_values
            .into_iter()
            .map(|resolved| {
                resolved.map_err(|err| match err {
                    #[cfg(feature = "expr-abort")]
                    ExpressionError::Abort { .. } => Terminate::Abort(err),
                    err @ ExpressionError::Error { .. } => Terminate::Error(err),
                })
            })
            .collect::<Vec<_>>();

        indices.extend(invalid_indices);
        resolved_values.extend(invalid_values);

        sort_resolved_values(&mut resolved_values, &indices);

        resolved_values
    }
}

fn sort_resolved_values(resolved_values: &mut Vec<Result<Value, Terminate>>, indices: &[usize]) {
    if !resolved_values.is_empty() {
        let base = resolved_values.as_ptr() as usize;
        let size = std::mem::size_of_val(&resolved_values[0]);
        resolved_values.sort_unstable_by(|a, b| {
            let position_a = ((a as *const _ as usize) - base) / size;
            let position_b = ((b as *const _ as usize) - base) / size;
            let index_a = indices[position_a];
            let index_b = indices[position_b];
            index_b.cmp(&index_a)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_resolved_values() {
        let mut resolved_values = vec![Ok("foo".into()), Ok("baz".into()), Ok("bar".into())];
        let indices = [0, 2, 1];

        sort_resolved_values(&mut resolved_values, &indices);

        assert_eq!(
            resolved_values,
            vec![Ok("foo".into()), Ok("bar".into()), Ok("baz".into())]
        )
    }
}
