use crate::{Condition, IndexHandle, TableRegistry, TableSearch};
use vrl_core::{
    diagnostic::{Label, Span},
    prelude::*,
};

#[derive(Debug)]
pub enum Error {
    TablesNotLoaded,
    InvalidCondition,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TablesNotLoaded => write!(f, "enrichment tables not loaded"),
            Error::InvalidCondition => write!(f, "invalid condition specified"),
        }
    }
}

impl std::error::Error for Error {}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        match self {
            Error::TablesNotLoaded => 111,
            Error::InvalidCondition => 112,
        }
    }

    fn labels(&self) -> Vec<Label> {
        match self {
            Error::TablesNotLoaded => {
                vec![Label::primary(
                    "enrichment table error: tables not loaded".to_string(),
                    Span::default(),
                )]
            }
            Error::InvalidCondition => {
                vec![Label::primary(
                    "enrichment table error: invalid condition specified".to_string(),
                    Span::default(),
                )]
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum ConditionToEvaluate {
    Equals {
        field: String,
        value: expression::Expr,
    },
    BetweenDates {
        field: String,
        from: expression::Expr,
        to: expression::Expr,
    },
}

impl ConditionToEvaluate {
    fn to_condition(&self, ctx: &mut Context) -> std::result::Result<Condition, ExpressionError> {
        Ok(match self {
            Self::Equals { field, value } => Condition::Equals {
                field: field.as_ref(),
                value: value.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned(),
            },
            Self::BetweenDates { field, from, to } => Condition::BetweenDates {
                field: field.as_ref(),
                from: from.resolve(ctx)?.try_timestamp()?,
                to: to.resolve(ctx)?.try_timestamp()?,
            },
        })
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
            .ok_or_else(|| Box::new(Error::TablesNotLoaded) as Box<dyn DiagnosticError>)?;

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

        /*
         An example of a search condition:
        {
            "field1": .value,
            "field2": { "from": .date1, "to": .date2 }
        }

        field1 is an exact search, field2 is searched against a date range.
        */

        let condition = arguments.required_object("condition")?;

        let condition = condition
            .iter()
            .map(|(field, expr)| {
                Ok(match expr {
                    expression::Expr::Container(expression::Container {
                        variant: expression::Variant::Object(object),
                    }) => ConditionToEvaluate::BetweenDates {
                        field: field.clone(),
                        from: object
                            .get("from")
                            .ok_or_else(|| {
                                Box::new(Error::InvalidCondition) as Box<dyn DiagnosticError>
                            })?
                            .clone(),
                        to: object
                            .get("to")
                            .ok_or_else(|| {
                                Box::new(Error::InvalidCondition) as Box<dyn DiagnosticError>
                            })?
                            .clone(),
                    },
                    _ => ConditionToEvaluate::Equals {
                        field: field.clone(),
                        value: expr.clone(),
                    },
                })
            })
            .collect::<std::result::Result<Vec<_>, Box<dyn DiagnosticError>>>()?;

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
    condition: Vec<ConditionToEvaluate>,
    index: Option<IndexHandle>,
    enrichment_tables: TableSearch,
}

impl Expression for GetEnrichmentTableRecordFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let condition = self
            .condition
            .iter()
            .map(|condition| condition.to_condition(ctx))
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
                    .filter_map(|condition| match condition {
                        // We can only index fields for an Equals condition
                        ConditionToEvaluate::Equals { field, .. } => Some(field.as_ref()),
                        _ => None,
                    })
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
    use super::*;
    use crate::test_util::get_table_registry;
    use shared::TimeZone;
    use std::collections::BTreeMap;

    #[test]
    fn find_table_row() {
        let registry = get_table_registry();
        let func = GetEnrichmentTableRecordFn {
            table: "dummy1".to_string(),
            condition: vec![ConditionToEvaluate::Equals {
                field: "field".to_string(),
                value: expression::Literal::from("value").into(),
            }],
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
            condition: vec![ConditionToEvaluate::Equals {
                field: "field".to_string(),
                value: expression::Literal::from("value").into(),
            }],
            index: None,
            enrichment_tables: registry.as_readonly(),
        };

        let mut compiler = state::Compiler::new();
        compiler.add_external_context(vec![Box::new(registry)]);

        assert_eq!(Ok(()), func.update_state(&mut compiler));
        assert_eq!(Some(IndexHandle(0)), func.index);
    }
}
