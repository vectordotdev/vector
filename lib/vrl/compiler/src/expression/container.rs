use crate::expression::{Array, Block, Group, Object, Resolved, Value};
use crate::{Context, Expression, State, TypeDef};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Container {
    pub variant: Variant,
}

impl Container {
    pub fn new(variant: Variant) -> Self {
        Self { variant }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Variant {
    Group(Group),
    Block(Block),
    Array(Array),
    Object(Object),
}

impl Expression for Container {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Variant::*;

        match &self.variant {
            Group(v) => v.resolve(ctx),
            Block(v) => v.resolve(ctx),
            Array(v) => v.resolve(ctx),
            Object(v) => v.resolve(ctx),
        }
    }

    fn as_value(&self) -> Option<Value> {
        use Variant::*;

        match &self.variant {
            Group(v) => v.as_value(),
            Block(v) => v.as_value(),
            Array(v) => v.as_value(),
            Object(v) => v.as_value(),
        }
    }

    fn type_def(&self, state: &State) -> TypeDef {
        use Variant::*;

        match &self.variant {
            Group(v) => v.type_def(state),
            Block(v) => v.type_def(state),
            Array(v) => v.type_def(state),
            Object(v) => v.type_def(state),
        }
    }

    fn dump(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        use Variant::*;

        match &self.variant {
            Group(v) => v.dump(vm),
            Block(v) => v.dump(vm),
            Array(v) => v.dump(vm),
            Object(v) => v.dump(vm),
        }
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(&self, ctx: &mut crate::llvm::Context<'ctx>) -> Result<(), String> {
        use Variant::*;

        match &self.variant {
            Group(v) => v.emit_llvm(ctx),
            Block(v) => v.emit_llvm(ctx),
            Array(v) => v.emit_llvm(ctx),
            Object(v) => v.emit_llvm(ctx),
        }
    }
}

impl fmt::Display for Container {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::*;

        match &self.variant {
            Group(v) => v.fmt(f),
            Block(v) => v.fmt(f),
            Array(v) => v.fmt(f),
            Object(v) => v.fmt(f),
        }
    }
}

impl From<Group> for Variant {
    fn from(group: Group) -> Self {
        Variant::Group(group)
    }
}

impl From<Block> for Variant {
    fn from(block: Block) -> Self {
        Variant::Block(block)
    }
}

impl From<Array> for Variant {
    fn from(array: Array) -> Self {
        Variant::Array(array)
    }
}

impl From<Object> for Variant {
    fn from(object: Object) -> Self {
        Variant::Object(object)
    }
}
