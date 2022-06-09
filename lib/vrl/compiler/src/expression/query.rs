use std::fmt;

use lookup::LookupBuf;
use value::{kind::remove, Kind, Value};

use crate::{
    expression::{Container, Resolved, Variable},
    parser::ast::Ident,
    state::{ExternalEnv, LocalEnv},
    type_def::Details,
    Context, Expression, TypeDef,
};

#[derive(Clone, PartialEq)]
pub struct Query {
    target: Target,
    path: LookupBuf,
}

impl Query {
    // TODO:
    // - error when trying to index into object
    // - error when trying to path into array
    pub fn new(target: Target, path: LookupBuf) -> Self {
        Query { target, path }
    }

    pub fn path(&self) -> &LookupBuf {
        &self.path
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn is_external(&self) -> bool {
        matches!(self.target, Target::External)
    }

    pub fn as_variable(&self) -> Option<&Variable> {
        match &self.target {
            Target::Internal(variable) => Some(variable),
            _ => None,
        }
    }

    pub fn variable_ident(&self) -> Option<&Ident> {
        match &self.target {
            Target::Internal(v) => Some(v.ident()),
            _ => None,
        }
    }

    pub fn expression_target(&self) -> Option<&dyn Expression> {
        match &self.target {
            Target::FunctionCall(expr) => Some(expr),
            Target::Container(expr) => Some(expr),
            _ => None,
        }
    }

    pub fn delete_type_def(
        &self,
        external: &mut ExternalEnv,
    ) -> Result<Option<Kind>, remove::Error> {
        let target = external.target_mut();
        let value = target.value.clone();
        let mut type_def = target.type_def.clone();

        let result = type_def.remove_at_path(
            &self.path.to_lookup(),
            remove::Strategy {
                coalesced_path: remove::CoalescedPath::Reject,
            },
        );

        external.update_target(Details { type_def, value });

        result
    }
}

impl Expression for Query {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Target::*;

        let value = match &self.target {
            External => {
                return Ok(ctx
                    .target()
                    .target_get(&self.path)
                    .ok()
                    .flatten()
                    .cloned()
                    .unwrap_or(Value::Null))
            }
            Internal(variable) => variable.resolve(ctx)?,
            FunctionCall(call) => call.resolve(ctx)?,
            Container(container) => container.resolve(ctx)?,
        };

        Ok(crate::Target::target_get(&value, &self.path)
            .ok()
            .flatten()
            .cloned()
            .unwrap_or(Value::Null))
    }

    fn as_value(&self) -> Option<Value> {
        match self.target {
            Target::Internal(ref variable) => variable
                .value()
                .and_then(|v| v.get_by_path(self.path()))
                .cloned(),
            _ => None,
        }
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        use Target::*;

        match &self.target {
            External => state
                .1
                .target()
                .clone()
                .type_def
                .at_path(&self.path.to_lookup()),
            Internal(variable) => variable.type_def(state).at_path(&self.path.to_lookup()),
            FunctionCall(call) => call.type_def(state).at_path(&self.path.to_lookup()),
            Container(container) => container.type_def(state).at_path(&self.path.to_lookup()),
        }
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.target {
            Target::Internal(_)
                if !self.path.is_root() && !self.path.iter().next().unwrap().is_index() =>
            {
                write!(f, "{}.{}", self.target, self.path)
            }
            _ => write!(f, "{}{}", self.target, self.path),
        }
    }
}

impl fmt::Debug for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Query({:?}, {:?})", self.target, self.path)
    }
}

#[derive(Clone, PartialEq)]
pub enum Target {
    Internal(Variable),
    External,

    #[cfg(feature = "expr-function_call")]
    FunctionCall(crate::expression::FunctionCall),
    #[cfg(not(feature = "expr-function_call"))]
    FunctionCall(crate::expression::Noop),
    Container(Container),
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::*;

        match self {
            Internal(v) => v.fmt(f),
            External => write!(f, "."),
            FunctionCall(v) => v.fmt(f),
            Container(v) => v.fmt(f),
        }
    }
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::*;

        match self {
            Internal(v) => write!(f, "Internal({:?})", v),
            External => f.write_str("External"),
            FunctionCall(v) => v.fmt(f),
            Container(v) => v.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state;

    #[test]
    fn test_type_def() {
        let query = Query {
            target: Target::External,
            path: LookupBuf::root(),
        };

        let state = (&state::LocalEnv::default(), &state::ExternalEnv::default());
        let type_def = query.type_def(state);

        assert!(type_def.is_infallible());
        assert!(type_def.is_object());

        let object = type_def.as_object().unwrap();

        assert!(object.is_any());
    }
}
