use core::ExpressionError;

use crate::expression::Block;
use crate::parser::{Ident, Node};
use crate::value::VrlValueConvert;
use crate::{Context, Expression, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionClosure {
    variables: Vec<Node<Ident>>,
    block: Block,
}

impl FunctionClosure {
    pub(crate) fn new(variables: Vec<Node<Ident>>, block: Block) -> Self {
        Self { variables, block }
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

        let key_ident = self
            .variables
            .get(0)
            .and_then(|v| (!v.is_empty()).then(|| v.as_ref()));

        let value_ident = self
            .variables
            .get(1)
            .and_then(|v| (!v.is_empty()).then(|| v.as_ref()));

        let state = ctx.state_mut();
        let old_key_data = key_ident.and_then(|ident| {
            state
                .swap_variable(ident.clone(), cloned_key.into())
                .map(|value| (ident.clone(), value))
        });
        let old_value_data = value_ident.and_then(|ident| {
            state
                .swap_variable(ident.clone(), cloned_value)
                .map(|value| (ident.clone(), value))
        });

        self.resolve(ctx)?;

        if let Some((ident, value)) = old_key_data {
            let state = ctx.state_mut();
            state.insert_variable(ident, value);
        }

        if let Some((ident, value)) = old_value_data {
            let state = ctx.state_mut();
            state.insert_variable(ident, value);
        }

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

        let index_ident = self
            .variables
            .get(0)
            .and_then(|v| (!v.is_empty()).then(|| v.as_ref()));

        let value_ident = self
            .variables
            .get(1)
            .and_then(|v| (!v.is_empty()).then(|| v.as_ref()));

        let state = ctx.state_mut();
        let old_index_data = index_ident.and_then(|ident| {
            state
                .swap_variable(ident.clone(), index.into())
                .map(|value| (ident.clone(), value))
        });
        let old_value_data = value_ident.and_then(|ident| {
            state
                .swap_variable(ident.clone(), cloned_value)
                .map(|value| (ident.clone(), value))
        });

        self.resolve(ctx)?;

        if let Some((ident, value)) = old_index_data {
            let state = ctx.state_mut();
            state.insert_variable(ident, value);
        }

        if let Some((ident, value)) = old_value_data {
            let state = ctx.state_mut();
            state.insert_variable(ident, value);
        }

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

        let ident = self
            .variables
            .get(0)
            .and_then(|v| (!v.is_empty()).then(|| v.as_ref()));

        let state = ctx.state_mut();
        let old_data = ident.and_then(|ident| {
            state
                .swap_variable(ident.clone(), cloned_key.into())
                .map(|value| (ident.clone(), value))
        });

        *key = self.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned();

        if let Some((ident, value)) = old_data {
            let state = ctx.state_mut();
            state.insert_variable(ident, value);
        }

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

        let ident = self
            .variables
            .get(0)
            .and_then(|v| (!v.is_empty()).then(|| v.as_ref()));

        let state = ctx.state_mut();
        let old_data = ident.and_then(|ident| {
            state
                .swap_variable(ident.clone(), cloned_value)
                .map(|value| (ident.clone(), value))
        });

        *value = self.resolve(ctx)?;

        if let Some((ident, value)) = old_data {
            let state = ctx.state_mut();
            state.insert_variable(ident, value);
        }

        Ok(())
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
