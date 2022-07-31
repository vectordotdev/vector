use std::{fmt, ptr::addr_of_mut};

use lookup::LookupBuf;
use value::{kind::remove, Kind, Value};

use crate::{
    expression::{Container, Resolved, Variable},
    parser::ast::Ident,
    state::{ExternalEnv, LocalEnv},
    type_def::Details,
    BatchContext, Context, Expression, TypeDef,
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
        use Target::{Container, External, FunctionCall, Internal};

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

        Ok(value
            .get_by_path(&self.path)
            .cloned()
            .unwrap_or(Value::Null))
    }

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        use Target::{Container, External, FunctionCall, Internal};

        match &mut self.target {
            External => {
                return for index in selection_vector {
                    let index = *index;
                    ctx.resolved_values[index] = Ok(ctx.targets[index]
                        .target_get(&self.path)
                        .ok()
                        .flatten()
                        .cloned()
                        .unwrap_or(Value::Null));
                };
            }
            Internal(variable) => variable.resolve_batch(ctx, selection_vector),
            FunctionCall(call) => call.resolve_batch(ctx, selection_vector),
            Container(container) => container.resolve_batch(ctx, selection_vector),
        };

        for index in selection_vector {
            let resolved = addr_of_mut!(ctx.resolved_values[*index]);
            let result = unsafe { resolved.read() }.map(|value| {
                value
                    .get_by_path(&self.path)
                    .cloned()
                    .unwrap_or(Value::Null)
            });

            unsafe { resolved.write(result) };
        }
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
        use Target::{Container, External, FunctionCall, Internal};

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

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        let query_begin_block = ctx.append_basic_block("query_begin");
        let query_end_block = ctx.append_basic_block("query_end");

        ctx.build_unconditional_branch(query_begin_block);
        ctx.position_at_end(query_begin_block);

        let result_ref = ctx.result_ref();
        let path_name = format!("{}", self.path);
        let path_ref = ctx
            .into_const(self.path.clone(), &path_name)
            .as_pointer_value();

        match &self.target {
            Target::External => {
                ctx.fns().vrl_expression_query_target_external.build_call(
                    ctx.builder(),
                    ctx.context_ref(),
                    ctx.cast_lookup_buf_ref_type(path_ref),
                    result_ref,
                );

                ctx.build_unconditional_branch(query_end_block);
                ctx.position_at_end(query_end_block);

                return Ok(());
            }
            Target::Internal(variable) => ctx.emit_llvm(
                variable,
                ctx.result_ref(),
                (state.0, state.1),
                query_end_block,
                vec![],
            )?,
            Target::FunctionCall(call) => ctx.emit_llvm(
                call,
                ctx.result_ref(),
                (state.0, state.1),
                query_end_block,
                vec![],
            )?,
            Target::Container(container) => ctx.emit_llvm(
                container,
                ctx.result_ref(),
                (state.0, state.1),
                query_end_block,
                vec![],
            )?,
        };

        let target_fallible = match &self.target {
            Target::Internal(_) | Target::External => false,
            Target::FunctionCall(call) => call.type_def((state.0, state.1)).is_fallible(),
            Target::Container(container) => container.type_def((state.0, state.1)).is_fallible(),
        };

        if target_fallible {
            let query_target_ok_block = ctx.append_basic_block("query_target_ok");

            let is_ok = ctx
                .fns()
                .vrl_resolved_is_ok
                .build_call(ctx.builder(), result_ref)
                .try_as_basic_value()
                .left()
                .expect("result is not a basic value")
                .try_into()
                .expect("result is not an int value");

            ctx.build_conditional_branch(is_ok, query_target_ok_block, query_end_block);
            ctx.position_at_end(query_target_ok_block);
        }

        ctx.fns().vrl_expression_query_target.build_call(
            ctx.builder(),
            ctx.cast_lookup_buf_ref_type(path_ref),
            result_ref,
        );
        ctx.build_unconditional_branch(query_end_block);

        ctx.position_at_end(query_end_block);

        Ok(())
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
        use Target::{Container, External, FunctionCall, Internal};

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
        use Target::{Container, External, FunctionCall, Internal};

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
