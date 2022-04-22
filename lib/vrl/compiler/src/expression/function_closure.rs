use core::ExpressionError;

use crate::expression::Block;
use crate::parser::Ident;
use crate::state::Runtime;
use crate::value::VrlValueConvert;
use crate::{Context, Expression, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionClosure {
    variables: Vec<Ident>,
    block: Block,
}

impl FunctionClosure {
    pub fn new<T: Into<Ident>>(variables: Vec<T>, block: Block) -> Self {
        Self {
            variables: variables.into_iter().map(Into::into).collect(),
            block,
        }
    }

    /// Run the closure to completion, given the provided key/value pair, and
    /// the runtime context.
    ///
    /// The provided values are *NOT* mutated during the run, and no return
    /// value is provided by this method. See `map_key` or `map_value` for
    /// mutating alternatives.
    pub fn run_key_value(
        &self,
        ctx: &mut Context,
        key: &str,
        value: &Value,
    ) -> Result<(), ExpressionError> {
        // TODO: we need to allow `LocalEnv` to take a muable reference to
        // values, instead of owning them.
        let cloned_key = key.to_owned();
        let cloned_value = value.clone();

        let key_ident = self.ident(0);
        let value_ident = self.ident(1);

        let old_key = insert(ctx.state_mut(), key_ident, cloned_key.into());
        let old_value = insert(ctx.state_mut(), value_ident, cloned_value);

        self.resolve(ctx)?;

        cleanup(ctx.state_mut(), key_ident, old_key);
        cleanup(ctx.state_mut(), value_ident, old_value);

        Ok(())
    }

    /// Run the closure to completion, given the provided index/value pair, and
    /// the runtime context.
    ///
    /// The provided values are *NOT* mutated during the run, and no return
    /// value is provided by this method. See `map_key` or `map_value` for
    /// mutating alternatives.
    pub fn run_index_value(
        &self,
        ctx: &mut Context,
        index: usize,
        value: &Value,
    ) -> Result<(), ExpressionError> {
        // TODO: we need to allow `LocalEnv` to take a muable reference to
        // values, instead of owning them.
        let cloned_value = value.clone();

        let index_ident = self.ident(0);
        let value_ident = self.ident(1);

        let old_index = insert(ctx.state_mut(), index_ident, index.into());
        let old_value = insert(ctx.state_mut(), value_ident, cloned_value);

        self.resolve(ctx)?;

        cleanup(ctx.state_mut(), index_ident, old_index);
        cleanup(ctx.state_mut(), value_ident, old_value);

        Ok(())
    }

    /// Run the closure to completion, given the provided key, and the runtime
    /// context.
    ///
    /// The provided key is *MUTATED* by overwriting the key with the return
    /// value of the closure after completion.
    ///
    /// See `run_key_value` and `run_index_value` for immutable alternatives.
    pub fn map_key(&self, ctx: &mut Context, key: &mut String) -> Result<(), ExpressionError> {
        // TODO: we need to allow `LocalEnv` to take a muable reference to
        // values, instead of owning them.
        let cloned_key = key.clone();
        let ident = self.ident(0);
        let old_key = insert(ctx.state_mut(), ident, cloned_key.into());

        *key = self.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned();

        cleanup(ctx.state_mut(), ident, old_key);

        Ok(())
    }

    /// Run the closure to completion, given the provided value, and the runtime
    /// context.
    ///
    /// The provided value is *MUTATED* by overwriting the value with the return
    /// value of the closure after completion.
    ///
    /// See `run_key_value` and `run_index_value` for immutable alternatives.
    pub fn map_value(&self, ctx: &mut Context, value: &mut Value) -> Result<(), ExpressionError> {
        // TODO: we need to allow `LocalEnv` to take a muable reference to
        // values, instead of owning them.
        let cloned_value = value.clone();
        let ident = self.ident(0);
        let old_value = insert(ctx.state_mut(), ident, cloned_value);

        *value = self.resolve(ctx)?;

        cleanup(ctx.state_mut(), ident, old_value);

        Ok(())
    }

    fn ident(&self, index: usize) -> Option<&Ident> {
        self.variables
            .get(index)
            .and_then(|v| (!v.is_empty()).then(|| v))
    }
}

fn insert(state: &mut Runtime, ident: Option<&Ident>, data: Value) -> Option<Value> {
    ident.and_then(|ident| state.swap_variable(ident.clone(), data))
}

fn cleanup(state: &mut Runtime, ident: Option<&Ident>, data: Option<Value>) {
    match (ident, data) {
        (Some(ident), Some(value)) => {
            state.insert_variable(ident.clone(), value);
        }
        (Some(ident), None) => state.remove_variable(ident),
        _ => {}
    }
}

impl Expression for FunctionClosure {
    fn resolve(&self, ctx: &mut Context) -> core::Resolved {
        // NOTE: It is the task of the caller to ensure the closure arguments
        // are inserted into the context before resolving this closure.
        self.block.resolve(ctx)
    }

    fn type_def(
        &self,
        state: (&crate::state::LocalEnv, &crate::state::ExternalEnv),
    ) -> crate::TypeDef {
        self.block.type_def(state)
    }
}
