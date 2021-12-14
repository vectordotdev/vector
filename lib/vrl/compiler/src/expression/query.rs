use crate::expression::{assignment, Container, FunctionCall, Resolved, Variable};
use crate::parser::ast::Ident;
use crate::{Context, Expression, State, TypeDef, Value};
use lookup::LookupBuf;
use std::collections::BTreeMap;
use std::fmt;

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

    pub fn delete_type_def(&self, state: &mut State) {
        if self.is_external() {
            if let Some(ref mut target) = state.target().as_mut() {
                let value = target.value.clone();
                let type_def = target.type_def.remove_path(&self.path);

                state.update_target(assignment::Details { type_def, value })
            }
        }
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

    fn as_value(&self) -> Option<Value> {
        match self.target {
            Target::Internal(ref variable) => variable
                .value()
                .and_then(|v| v.get_by_path(self.path()))
                .cloned(),
            _ => None,
        }
    }

    fn type_def(&self, state: &State) -> TypeDef {
        use Target::*;

        match &self.target {
            External => {
                // `.` path must be an object
                //
                // TODO: make sure to enforce this
                if self.path.is_root() {
                    return TypeDef::new()
                        .object::<String, TypeDef>(BTreeMap::default())
                        .infallible();
                }

                match state.target() {
                    None => TypeDef::new().unknown().infallible(),
                    Some(details) => details.clone().type_def.at_path(self.path.clone()),
                }
            }

            Internal(variable) => variable.type_def(state).at_path(self.path.clone()),
            FunctionCall(call) => call.type_def(state).at_path(self.path.clone()),
            Container(container) => container.type_def(state).at_path(self.path.clone()),
        }
    }

    fn dump(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        vm.write_chunk(crate::vm::OpCode::GetPath);
        let variable = match self.target {
            Target::External => crate::vm::Variable::External(self.path.clone()),
            _ => unimplemented!("Only external vars for now"),
        };
        let target = vm.get_target(&variable);
        vm.write_primitive(target);
        Ok(())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: &crate::state::Compiler,
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        let result_ref = ctx.result_ref();
        let path_name = format!("{}", self.path);
        let path_ref = ctx
            .into_const(self.path.clone(), &path_name)
            .as_pointer_value();

        match &self.target {
            Target::External => {
                let fn_ident = "vrl_expression_query_target_external_impl";
                let fn_impl = ctx
                    .module()
                    .get_function(fn_ident)
                    .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                ctx.builder().build_call(
                    fn_impl,
                    &[
                        ctx.context_ref().into(),
                        ctx.builder()
                            .build_bitcast(
                                path_ref,
                                fn_impl
                                    .get_nth_param(1)
                                    .unwrap()
                                    .get_type()
                                    .into_pointer_type(),
                                "cast",
                            )
                            .into(),
                        result_ref.into(),
                    ],
                    fn_ident,
                );

                return Ok(());
            }
            Target::Internal(variable) => variable.emit_llvm(state, ctx)?,
            Target::FunctionCall(call) => call.emit_llvm(state, ctx)?,
            Target::Container(container) => container.emit_llvm(state, ctx)?,
        };

        let fn_ident = "vrl_expression_query_target_impl";
        let fn_impl = ctx
            .module()
            .get_function(fn_ident)
            .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
        ctx.builder()
            .build_call(fn_impl, &[path_ref.into(), result_ref.into()], fn_ident);

        Ok(())
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

#[derive(Clone, PartialEq)]
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
