use crate::expression::Block;
use crate::parser::{Ident, Node};
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

    pub fn key_value(&self, key: String, value: Value) -> FunctionClosureKeyValue {
        let key = self
            .variables
            .get(0)
            .and_then(|v| (!v.is_empty()).then(|| (v.clone().into_inner(), key)));

        FunctionClosureKeyValue {
            key,
            value: self.value(value),
            block: &self.block,
        }
    }

    pub fn index_value(&self, index: usize, value: Value) -> FunctionClosureIndexValue {
        let index = self
            .variables
            .get(0)
            .and_then(|v| (!v.is_empty()).then(|| (v.clone().into_inner(), index)));

        FunctionClosureIndexValue {
            index,
            value: self.value(value),
            block: &self.block,
        }
    }

    fn value(&self, value: Value) -> Option<(Ident, Value)> {
        self.variables
            .get(1)
            .and_then(|v| (!v.is_empty()).then(|| (v.clone().into_inner(), value)))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionClosureKeyValue<'a> {
    key: Option<(Ident, String)>,
    value: Option<(Ident, Value)>,
    block: &'a Block,
}

impl Expression for FunctionClosureKeyValue<'_> {
    fn resolve(&self, ctx: &mut Context) -> core::Resolved {
        let state = ctx.state_mut();

        // Make sure we keep track of parent scopes using the same variable
        // names as the closure. Once the closure ends, we restore the old state
        // for these variables.
        let old_key_value = self.key.as_ref().and_then(|(ident, key)| {
            state
                .swap_variable(ident.clone(), key.clone().into())
                .map(move |v| (ident, v))
        });

        let old_value_value = self.value.as_ref().and_then(|(ident, value)| {
            state
                .swap_variable(ident.clone(), value.clone())
                .map(move |v| (ident, v))
        });

        let value = self.block.resolve(ctx)?;

        let state = ctx.state_mut();

        if let Some((ident, value)) = old_key_value {
            state.insert_variable(ident.clone(), value);
        }

        if let Some((ident, value)) = old_value_value {
            state.insert_variable(ident.clone(), value);
        }

        Ok(value)
    }

    fn type_def(
        &self,
        state: (&crate::state::LocalEnv, &crate::state::ExternalEnv),
    ) -> crate::TypeDef {
        self.block.type_def(state)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionClosureIndexValue<'a> {
    index: Option<(Ident, usize)>,
    value: Option<(Ident, Value)>,
    block: &'a Block,
}

impl Expression for FunctionClosureIndexValue<'_> {
    fn resolve(&self, ctx: &mut Context) -> core::Resolved {
        let state = ctx.state_mut();

        // Make sure we keep track of parent scopes using the same variable
        // names as the closure. Once the closure ends, we restore the old state
        // for these variables.
        let old_index_value = self.index.as_ref().and_then(|(ident, index)| {
            state
                .swap_variable(ident.clone(), (*index).into())
                .map(move |v| (ident, v))
        });

        let old_value_value = self.value.as_ref().and_then(|(ident, value)| {
            state
                .swap_variable(ident.clone(), value.clone())
                .map(move |v| (ident, v))
        });

        let value = self.block.resolve(ctx)?;

        let state = ctx.state_mut();

        if let Some((ident, value)) = old_index_value {
            state.insert_variable(ident.clone(), value);
        }

        if let Some((ident, value)) = old_value_value {
            state.insert_variable(ident.clone(), value);
        }

        Ok(value)
    }

    fn type_def(
        &self,
        state: (&crate::state::LocalEnv, &crate::state::ExternalEnv),
    ) -> crate::TypeDef {
        self.block.type_def(state)
    }
}
