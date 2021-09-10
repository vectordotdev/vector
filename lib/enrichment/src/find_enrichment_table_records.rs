use crate::{
    vrl_util::{self, add_index},
    Case, Condition, IndexHandle, TableRegistry, TableSearch,
};
use std::collections::BTreeMap;
use vrl_core::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct FindEnrichmentTableRecords;
impl Function for FindEnrichmentTableRecords {
    fn identifier(&self) -> &'static str {
        "find_enrichment_table_records"
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
            Parameter {
                keyword: "case_sensitive",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[]
    }

    fn compile(&self, state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let registry = state
            .get_external_context::<TableRegistry>()
            .ok_or(Box::new(vrl_util::Error::TablesNotLoaded) as Box<dyn DiagnosticError>)?;

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

        let case_sensitive = arguments
            .optional_literal("case_sensitive")?
            .map(|literal| literal.to_value().try_boolean())
            .transpose()
            .expect("case_sensitive should be boolean") // This will have been caught by the type checker.
            .map(|case_sensitive| {
                if case_sensitive {
                    Case::Sensitive
                } else {
                    Case::Insensitive
                }
            })
            .unwrap_or(Case::Sensitive);

        Ok(Box::new(FindEnrichmentTableRecordsFn {
            table,
            condition,
            index: None,
            case_sensitive,
            enrichment_tables: registry.as_readonly(),
        }))
    }
}

#[derive(Debug, Clone)]
pub struct FindEnrichmentTableRecordsFn {
    table: String,
    condition: BTreeMap<String, expression::Expr>,
    index: Option<IndexHandle>,
    case_sensitive: Case,
    enrichment_tables: TableSearch,
}

impl Expression for FindEnrichmentTableRecordsFn {
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
            .find_table_rows(&self.table, self.case_sensitive, &condition, self.index)?
            .into_iter()
            .map(Value::Object)
            .collect();

        Ok(Value::Array(data))
    }

    fn update_state(
        &mut self,
        state: &mut state::Compiler,
    ) -> std::result::Result<(), ExpressionError> {
        self.index = Some(add_index(
            state,
            &self.table,
            self.case_sensitive,
            &self.condition,
        )?);
        Ok(())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .infallible()
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
        let func = FindEnrichmentTableRecordsFn {
            table: "dummy1".to_string(),
            condition: btreemap! {
                "field" =>  expression::Literal::from("value"),
            },
            index: Some(IndexHandle(999)),
            case_sensitive: Case::Sensitive,
            enrichment_tables: registry.as_readonly(),
        };

        let tz = TimeZone::default();
        let mut object: Value = BTreeMap::new().into();
        let mut runtime_state = vrl_core::state::Runtime::default();
        let mut ctx = Context::new(&mut object, &mut runtime_state, &tz);

        registry.finish_load();

        let got = func.resolve(&mut ctx);

        assert_eq!(Ok(value![vec![value!({ "field": "result" })]]), got);
    }

    #[test]
    fn add_indexes() {
        let registry = get_table_registry();

        let mut func = FindEnrichmentTableRecordsFn {
            table: "dummy1".to_string(),
            condition: btreemap! {
                "field" =>  expression::Literal::from("value"),
            },
            index: None,
            case_sensitive: Case::Sensitive,
            enrichment_tables: registry.as_readonly(),
        };

        let mut compiler = state::Compiler::new();
        compiler.set_external_context(Some(Box::new(registry)));

        assert_eq!(Ok(()), func.update_state(&mut compiler));
        assert_eq!(Some(IndexHandle(0)), func.index);
    }
}
