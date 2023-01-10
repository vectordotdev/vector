use crate::{
    expression::{Container, Resolved, Variable},
    parser::ast::Ident,
    state::ExternalEnv,
    state::{TypeInfo, TypeState},
    type_def::Details,
    Context, Expression,
};
use lookup::{OwnedTargetPath, OwnedValuePath, PathPrefix};
use std::fmt;
use value::Value;

#[derive(Clone, PartialEq)]
pub struct Query {
    target: Target,
    path: OwnedValuePath,
}

impl Query {
    // TODO:
    // - error when trying to index into object
    // - error when trying to path into array
    pub fn new(target: Target, path: OwnedValuePath) -> Self {
        Query { target, path }
    }

    pub fn path(&self) -> &OwnedValuePath {
        &self.path
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn is_external(&self) -> bool {
        matches!(self.target, Target::External(_))
    }

    pub fn external_path(&self) -> Option<OwnedTargetPath> {
        match self.target {
            Target::External(prefix) => Some(OwnedTargetPath {
                prefix,
                path: self.path.clone(),
            }),
            _ => None,
        }
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

    // Only "external" paths are supported. Non external paths are ignored
    // see: https://github.com/vectordotdev/vector/issues/11246
    pub fn delete_type_def(&self, external: &mut ExternalEnv, compact: bool) {
        if let Some(target_path) = self.external_path() {
            match target_path.prefix {
                PathPrefix::Event => {
                    let mut type_def = external.target().type_def.clone();
                    let _ = type_def.remove(&target_path.path, compact);
                    external.update_target(Details {
                        type_def,
                        value: None,
                    });
                }
                PathPrefix::Metadata => {
                    let mut kind = external.metadata_kind().clone();
                    let _ = kind.remove(&target_path.path, compact);
                    external.update_metadata(kind);
                }
            }
        }
    }
}

impl Expression for Query {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Target::{Container, External, FunctionCall, Internal};

        let value = match &self.target {
            External(prefix) => {
                let path = OwnedTargetPath {
                    prefix: *prefix,
                    path: self.path.clone(),
                };
                return Ok(ctx
                    .target()
                    .target_get(&path)
                    .ok()
                    .flatten()
                    .cloned()
                    .unwrap_or(Value::Null));
            }
            Internal(variable) => variable.resolve(ctx)?,
            FunctionCall(call) => call.resolve(ctx)?,
            Container(container) => container.resolve(ctx)?,
        };

        Ok(value.get(&self.path).cloned().unwrap_or(Value::Null))
    }

    fn as_value(&self) -> Option<Value> {
        match self.target {
            Target::Internal(ref variable) => {
                variable.value().and_then(|v| v.get(self.path())).cloned()
            }
            _ => None,
        }
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        use Target::{Container, External, FunctionCall, Internal};

        let result = match &self.target {
            External(prefix) => state
                .external
                .kind(*prefix)
                .at_path(&self.path.clone())
                .into(),
            Internal(variable) => variable.type_def(state).at_path(&self.path.clone().into()),
            FunctionCall(call) => call.type_def(state).at_path(&self.path.clone().into()),
            Container(container) => container.type_def(state).at_path(&self.path.clone().into()),
        };

        TypeInfo::new(state, result)
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.target {
            Target::Internal(_)
                if !self.path.is_root() && !self.path.segments.first().unwrap().is_index() =>
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
    External(PathPrefix),

    #[cfg(feature = "expr-function_call")]
    FunctionCall(crate::expression::FunctionCall),
    #[cfg(not(feature = "expr-function_call"))]
    FunctionCall(crate::expression::Noop),
    Container(Container),
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::{Container, External, FunctionCall, Internal};

        match self {
            Internal(v) => v.fmt(f),
            External(prefix) => match prefix {
                PathPrefix::Event => write!(f, "."),
                PathPrefix::Metadata => write!(f, "@"),
            },
            FunctionCall(v) => v.fmt(f),
            Container(v) => v.fmt(f),
        }
    }
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::{Container, External, FunctionCall, Internal};

        match self {
            Internal(v) => write!(f, "Internal({v:?})"),
            External(prefix) => match prefix {
                PathPrefix::Event => f.write_str("External(Event)"),
                PathPrefix::Metadata => f.write_str("External(Metadata)"),
            },
            FunctionCall(v) => v.fmt(f),
            Container(v) => v.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_def() {
        let query = Query {
            target: Target::External(PathPrefix::Event),
            path: OwnedValuePath::root(),
        };

        let state = TypeState::default();
        let type_def = query.type_info(&state).result;

        assert!(type_def.is_infallible());
        assert!(type_def.is_object());

        let object = type_def.as_object().unwrap();

        assert!(object.is_any());
    }
}
