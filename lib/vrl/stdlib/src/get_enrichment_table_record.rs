use std::collections::BTreeMap;
use vrl::{enrichment::Condition, prelude::*};

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

        let table = arguments.required_enum("table", &tables)?.to_string();
        let condition = arguments.required_object("condition")?;

        Ok(Box::new(GetEnrichmentTableRecordFn { table, condition }))
    }
}

#[derive(Debug, Clone)]
pub struct GetEnrichmentTableRecordFn {
    table: String,
    condition: BTreeMap<String, expression::Expr>,
}

impl Expression for GetEnrichmentTableRecordFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let condition = self
            .condition
            .iter()
            .map(|(key, value)| {
                Ok(Condition::Equals {
                    field: key,
                    value: value.resolve(ctx)?.try_bytes_utf8_lossy()?.to_string(),
                })
            })
            .collect::<Result<Vec<Condition>>>()?;

        let tables = ctx
            .get_enrichment_tables()
            .ok_or("enrichment tables not loaded")?;

        match tables.find_table_row(&self.table, &condition)? {
            None => Err("data not found".into()),
            Some(data) => Ok(Value::Object(data)),
        }
    }

    fn update_state(
        &self,
        state: &mut state::Compiler,
    ) -> std::result::Result<(), ExpressionError> {
        match state.get_enrichment_tables_mut() {
            Some(ref mut table) => {
                let fields = self
                    .condition
                    .iter()
                    .map(|(field, _)| field.as_ref())
                    .collect::<Vec<_>>();
                table.add_index(&self.table, &fields)?;
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
    use super::*;
    use shared::{btreemap, TimeZone};
    use vrl::enrichment;

    #[derive(Clone, Debug)]
    struct DummyEnrichmentTable;

    impl enrichment::TableSetup for DummyEnrichmentTable {
        fn table_ids(&self) -> Vec<String> {
            vec!["table".to_string()]
        }

        fn add_index(&mut self, table: &str, fields: &[&str]) -> std::result::Result<(), String> {
            assert_eq!("table", table);
            assert_eq!(vec!["field"], fields);

            Ok(())
        }

        fn as_readonly(&self) -> Box<dyn enrichment::TableSearch + Send + Sync> {
            Box::new(self.clone())
        }
    }

    impl enrichment::TableSearch for DummyEnrichmentTable {
        fn find_table_row<'a>(
            &self,
            table: &str,
            condition: &'a [Condition<'a>],
        ) -> std::result::Result<Option<BTreeMap<String, Value>>, String> {
            assert_eq!(table, "table");
            assert_eq!(
                condition,
                vec![Condition::Equals {
                    field: "field",
                    value: "value".to_string(),
                }]
            );

            Ok(Some(btreemap! {
                "field" => Value::from("value"),
                "field2" => Value::from("value2"),
            }))
        }
    }

    #[test]
    fn find_table_row() {
        let func = GetEnrichmentTableRecordFn {
            table: "table".to_string(),
            condition: btreemap! {
                "field" =>  expression::Literal::from("value"),
            },
        };

        let tz = TimeZone::default();
        let enrichment_tables =
            Some(&DummyEnrichmentTable as &(dyn vrl::enrichment::TableSearch + Send + Sync));

        let mut object: Value = BTreeMap::new().into();
        let mut runtime_state = vrl::state::Runtime::default();
        let mut ctx = Context::new(&mut object, &mut runtime_state, &tz, enrichment_tables);

        let got = func.resolve(&mut ctx);

        assert_eq!(Ok(value! ({ "field": "value", "field2": "value2" })), got);
    }

    #[test]
    fn add_indexes() {
        let func = GetEnrichmentTableRecordFn {
            table: "table".to_string(),
            condition: btreemap! {
                "field" =>  expression::Literal::from("value"),
            },
        };

        let mut compiler =
            state::Compiler::new_with_enrichment_tables(Box::new(DummyEnrichmentTable));

        assert_eq!(Ok(()), func.update_state(&mut compiler));
    }
}
