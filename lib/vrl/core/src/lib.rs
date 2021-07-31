pub mod prelude;
mod runtime;

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

pub use compiler::{
    function, state, type_def::Index, value, Context, Expression, Function, Program, Target, Value,
};
pub use diagnostic;
pub use runtime::{Runtime, RuntimeResult, Terminate};

type EnrichmentTables = HashMap<String, Arc<RwLock<Box<dyn EnrichmentTable>>>>;

/// Compile a given source into the final [`Program`].
pub fn compile(
    source: &str,
    enrichment_tables: EnrichmentTables,
    fns: &[Box<dyn Function>],
) -> compiler::Result {
    let mut state = state::Compiler::default();

    for (table, data) in enrichment_tables {
        state.insert_variable(
            Ident(table.clone()),
            crate::expression::assignment::Details {
                type_def: TypeDef {
                    fallible: false,
                    kind: TypeKind::EnrichmentTable.into(),
                },
                value: None,
            },
        );
    }

    compile_with_state(source, fns, &mut state)
}

pub fn compile_with_state(
    source: &str,
    fns: &[Box<dyn Function>],
    state: &mut state::Compiler,
) -> compiler::Result {
    let ast = parser::parse(source).map_err(|err| vec![Box::new(err) as _])?;
    compiler::compile_with_state(ast, fns, state)
}
