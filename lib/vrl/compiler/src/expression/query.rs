use std::{collections::BTreeMap, fmt};

use lookup::LookupBuf;

use crate::{
    expression::{assignment, Container, FunctionCall, Resolved, Variable},
    parser::ast::Ident,
    vm::{self, OpCode},
    Context, Expression, State, TypeDef, Value,
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

    fn compile_to_vm(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        // Write the target depending on what target we are trying to retrieve.
        let variable = match &self.target {
            Target::External => {
                vm.write_opcode(OpCode::GetPath);
                vm::Variable::External(self.path.clone())
            }
            Target::Internal(variable) => {
                vm.write_opcode(OpCode::GetPath);
                vm::Variable::Internal(variable.ident().clone(), Some(self.path.clone()))
            }
            Target::FunctionCall(call) => {
                // Write the code to call the function.
                call.compile_to_vm(vm)?;

                // Then retrieve the given path from the returned value that has been pushed on the stack
                vm.write_opcode(OpCode::GetPath);
                vm::Variable::Stack(self.path.clone())
            }
            Target::Container(container) => {
                // Write the code to create the container onto the stack.
                container.compile_to_vm(vm)?;

                // Then retrieve the given path from the returned value that has been pushed on the stack
                vm.write_opcode(OpCode::GetPath);
                vm::Variable::Stack(self.path.clone())
            }
        };

        let target = vm.get_target(&variable);
        vm.write_primitive(target);

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
