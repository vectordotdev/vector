use crate::{Condition, IndexHandle, TableRegistry, TableSearch};
use std::collections::BTreeMap;
use vrl_core::{
    diagnostic::{Label, Span},
    prelude::*,
};

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

#[derive(Clone, Copy, Debug)]
pub struct GetEnrichmentTableRecord;
impl Function for GetEnrichmentTableRecord {
    fn identifier(&self) -> &'static str {
        "get_enrichment_table_record"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "table",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "condition",
                kind: kind::OBJECT,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[]
    }

    fn compile(&self, state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let registry = state
            .get_external_context::<TableRegistry>()
            .ok_or(Box::new(Error::TablesNotLoaded) as Box<dyn DiagnosticError>)?;

        let tables = registry
            .table_ids()
            .into_iter()
            .map(Value::from)
            .collect::<Vec<_>>();

        let table = arguments
            .required_enum("table", &tables)?
            .try_bytes_utf8_lossy()
            .expect("table is not valid utf8")
            .into_owned();
        let condition = arguments.required_object("condition")?;

        Ok(Box::new(GetEnrichmentTableRecordFn {
            table,
            condition,
            index: None,
            enrichment_tables: registry.as_readonly(),
        }))
    }
}

#[derive(Debug, Clone)]
pub struct GetEnrichmentTableRecordFn {
    table: String,
    condition: BTreeMap<String, expression::Expr>,
    index: Option<IndexHandle>,
    enrichment_tables: TableSearch,
}

impl Expression for GetEnrichmentTableRecordFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let condition = self
            .condition
            .iter()
            .map(|(key, value)| {
                Ok(Condition::Equals {
                    field: key,
                    value: value.resolve(ctx)?,
                })
            })
            .collect::<Result<Vec<Condition>>>()?;

        let data = self
            .enrichment_tables
            .find_table_row(&self.table, &condition, self.index)?;

        Ok(Value::Object(data))
    }

    fn update_state(
        &mut self,
        state: &mut state::Compiler,
    ) -> std::result::Result<(), ExpressionError> {
        let mut registry = state.get_external_context_mut::<TableRegistry>();

        match registry {
            Some(ref mut table) => {
                let fields = self
                    .condition
                    .iter()
                    .map(|(field, _)| field.as_ref())
                    .collect::<Vec<_>>();
                let index = table.add_index(&self.table, &fields)?;

                // Store the index to use while searching.
                self.index = Some(index);

                Ok(())
            }
            // We shouldn't reach this point since the type checker will ensure the table exists before this function is called.
            None => unreachable!("enrichment tables aren't loaded"),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .fallible()
            .add_object::<(), Kind>(map! { (): Kind::Bytes })
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::get_table_registry;

    use super::*;
    use shared::{btreemap, TimeZone};

    #[test]
    fn find_table_row() {
        let registry = get_table_registry();
        let func = GetEnrichmentTableRecordFn {
            table: "dummy1".to_string(),
            condition: btreemap! {
                "field" =>  expression::Literal::from("value"),
            },
            index: Some(IndexHandle(999)),
            enrichment_tables: registry.as_readonly(),
        };

        let tz = TimeZone::default();
        let mut object: Value = BTreeMap::new().into();
        let mut runtime_state = vrl_core::state::Runtime::default();
        let mut ctx = Context::new(&mut object, &mut runtime_state, &tz);

        registry.finish_load();

        let got = func.resolve(&mut ctx);

        assert_eq!(Ok(value! ({ "field": "result" })), got);
    }

    #[test]
    fn add_indexes() {
        let registry = get_table_registry();

        let mut func = GetEnrichmentTableRecordFn {
            table: "dummy1".to_string(),
            condition: btreemap! {
                "field" =>  expression::Literal::from("value"),
            },
            index: None,
            enrichment_tables: registry.as_readonly(),
        };

        let mut compiler = state::Compiler::new();
        compiler.set_external_context(Some(Box::new(registry)));

        assert_eq!(Ok(()), func.update_state(&mut compiler));
        assert_eq!(Some(IndexHandle(0)), func.index);
    }
}
