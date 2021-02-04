use crate::expression::{assignment, Container, FunctionCall, Resolved, Variable};
use crate::{Context, Expression, Path, State, TypeDef, Value};
use std::fmt;

#[derive(PartialEq)]
pub struct Query {
    target: Target,
    path: Path,
}

impl Query {
    // TODO:
    // - error when trying to index into object
    // - error when trying to path into array
    pub(crate) fn new(target: Target, path: Path) -> Self {
        Query { target, path }
    }
}

impl Expression for Query {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Target::*;

        let value = match &self.target {
            External => {
                return Ok(ctx
                    .target()
                    .get(&self.path)
                    .ok()
                    .flatten()
                    .unwrap_or(Value::Null))
            }
            Internal(variable) => variable.resolve(ctx)?,
            FunctionCall(call) => call.resolve(ctx)?,
            Container(container) => container.resolve(ctx)?,
        };

        Ok(crate::Target::get(&value, &self.path)
            .ok()
            .flatten()
            .unwrap_or(Value::Null))
    }

    fn type_def(&self, state: &State) -> TypeDef {
        use Target::*;

        match &self.target {
            External => {
                // `.` path must be an object
                //
                // TODO: make sure to enforce this
                if self.path.is_root() {
                    return TypeDef::new().object(None).infallible();
                }

                let target = assignment::Target::External(Some(self.path.clone()));

                match state.assignment(&target) {
                    None => TypeDef::new().any().infallible(),
                    Some(type_def) => type_def.clone(),
                }
            }

            // FIXME:
            //
            // Here we need to recursively get the inner type definitions to
            // check the final type definition.
            Internal(variable) => variable.type_def(state),
            FunctionCall(call) => call.type_def(state),
            Container(container) => container.type_def(state),
        }
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.target, self.path)
    }
}

impl fmt::Debug for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Query({:?}, {:?})", self.target, self.path)
    }
}

#[derive(PartialEq)]
pub enum Target {
    Internal(Variable),
    External,
    FunctionCall(FunctionCall),
    Container(Container),
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::*;

        match self {
            Internal(v) => v.fmt(f),
            External => Ok(()),
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
