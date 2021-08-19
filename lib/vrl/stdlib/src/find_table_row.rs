use std::collections::BTreeMap;
use vrl::{prelude::*, Condition, IndexHandle};

#[derive(Clone, Copy, Debug)]
pub struct FindTableRow;

impl Function for FindTableRow {
    fn identifier(&self) -> &'static str {
        "find_table_row"
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
        let tables = state
            .get_enrichment_tables()
            .as_ref()
            .map(|tables| {
                tables
                    .table_ids()
                    .into_iter()
                    .map(Value::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(Vec::new);

        let table = arguments
            .required_enum("table", &tables)?
            .try_bytes_utf8_lossy()
            .expect("table is not valid utf8")
            .into_owned();
        let condition = arguments.required_object("condition")?;

        Ok(Box::new(FindTableRowFn {
            table,
            condition,
            index: None,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct FindTableRowFn {
    table: String,
    condition: BTreeMap<String, expression::Expr>,
    index: Option<IndexHandle>,
}

impl Expression for FindTableRowFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let condition = self
            .condition
            .iter()
            .map(|(key, value)| {
                Ok(Condition::Equals {
                    field: key.to_string(),
                    value: value.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned(),
                })
            })
            .collect::<Result<Vec<Condition>>>()?;

        let tables = ctx
            .get_enrichment_tables()
            .as_ref()
            .ok_or("enrichment tables not loaded")?;

        let data = tables.find_table_row(&self.table, condition, self.index)?;
        Ok(Value::Object(data))
    }

    fn update_state(
        &mut self,
        state: &mut state::Compiler,
    ) -> std::result::Result<(), ExpressionError> {
        match state.get_enrichment_tables_mut() {
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
            None => Err("enrichment tables aren't loaded".into()),
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
    use super::*;
    use shared::{btreemap, TimeZone};
    use vrl::{Condition, EnrichmentTableSearch, EnrichmentTableSetup};

    #[derive(Clone)]
    struct DummyEnrichmentTable;

    impl EnrichmentTableSetup for DummyEnrichmentTable {
        fn table_ids(&self) -> Vec<String> {
            vec!["table".to_string()]
        }

        fn add_index(
            &mut self,
            table: &str,
            fields: &[&str],
        ) -> std::result::Result<IndexHandle, String> {
            assert_eq!("table", table);
            assert_eq!(vec!["field"], fields);

            Ok(IndexHandle(999))
        }
    }

    impl EnrichmentTableSearch for DummyEnrichmentTable {
        fn find_table_row(
            &self,
            table: &str,
            condition: Vec<Condition>,
            index: Option<IndexHandle>,
        ) -> std::result::Result<BTreeMap<String, Value>, String> {
            assert_eq!(table, "table");
            assert_eq!(
                condition,
                vec![Condition::Equals {
                    field: "field".to_string(),
                    value: "value".to_string(),
                }]
            );
            assert_eq!(index, Some(IndexHandle(999)));

            Ok(btreemap! {
                "field".to_string() => "value".to_string(),
                "field2".to_string() => "value2".to_string(),
            })
        }
    }

    #[test]
    fn find_table_row() {
        let func = FindTableRowFn {
            table: "table".to_string(),
            condition: btreemap! {
                "field" =>  expression::Literal::from("value"),
            },
            index: Some(IndexHandle(999)),
        };

        let tz = TimeZone::default();
        let enrichment_tables =
            Some(Box::new(DummyEnrichmentTable) as Box<dyn vrl::EnrichmentTableSearch>);

        let mut object: Value = BTreeMap::new().into();
        let mut runtime_state = vrl::state::Runtime::default();
        let mut ctx = Context::new(&mut object, &mut runtime_state, &tz, &enrichment_tables);

        let got = func.resolve(&mut ctx);

        assert_eq!(
            Ok(Value::from(btreemap! {
                "field" => Value::from("value"),
                "field2" => Value::from("value2"),
            })),
            got
        );
    }

    #[test]
    fn add_indexes() {
        let mut func = FindTableRowFn {
            table: "table".to_string(),
            condition: btreemap! {
                "field" =>  expression::Literal::from("value"),
            },
            index: None,
        };

        let mut compiler =
            state::Compiler::new_with_enrichment_tables(Box::new(DummyEnrichmentTable));

        assert_eq!(Ok(()), func.update_state(&mut compiler));
        assert_eq!(Some(IndexHandle(999)), func.index);
    }
}
