//! Utilities shared between both VRL functions.
use std::collections::BTreeMap;

use vrl_core::{
    diagnostic::{Label, Span},
    prelude::*,
};

use crate::{IndexHandle, TableRegistry};

#[derive(Debug)]
pub enum Error {
    TablesNotLoaded,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TablesNotLoaded => write!(f, "enrichment tables not loaded"),
        }
    }
}

impl std::error::Error for Error {}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        111
    }

    fn labels(&self) -> Vec<Label> {
        match self {
            Error::TablesNotLoaded => {
                vec![Label::primary(
                    "enrichment table error: tables not loaded".to_string(),
                    Span::default(),
                )]
            }
        }
    }
}

/// Add an index for the given condition to the given enrichment table.
pub(crate) fn add_index(
    state: &mut state::Compiler,
    tablename: &str,
    condition: &BTreeMap<String, expression::Expr>,
) -> std::result::Result<IndexHandle, ExpressionError> {
    let mut registry = state.get_external_context_mut::<TableRegistry>();

    match registry {
        Some(ref mut table) => {
            let fields = condition
                .iter()
                .map(|(field, _)| field.as_ref())
                .collect::<Vec<_>>();
            let index = table.add_index(tablename, &fields)?;

            Ok(index)
        }
        // We shouldn't reach this point since the type checker will ensure the table exists before this function is called.
        None => unreachable!("enrichment tables aren't loaded"),
    }
}
