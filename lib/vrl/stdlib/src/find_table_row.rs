use std::collections::BTreeMap;
use vrl::prelude::*;

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
                kind: kind::ENRICHMENT_TABLE,
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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let table = arguments.required_enrichment_table("table")?;
        let condition = arguments
            .required_object("condition")?
            .into_iter()
            .map(|(key, expr)| {
                Ok((
                    key,
                    expr.as_value()
                        .ok_or(vrl::function::Error::ExpectedStaticExpression {
                            keyword: "condition",
                            expr,
                        })
                        .map(|value| value.to_string())?,
                ))
            })
            .collect::<std::result::Result<BTreeMap<String, String>, vrl::function::Error>>()?;

        Ok(Box::new(FindTableRowFn { table, condition }))
    }
}

#[derive(Debug, Clone)]
pub struct FindTableRowFn {
    table: String,
    condition: BTreeMap<String, String>,
}

impl Expression for FindTableRowFn {
    fn resolve(&self, _ctx: &mut Context) -> Resolved {
        Ok(Value::Null)
    }

    fn update_state(&self, state: &mut state::Compiler) {
        match state.get_enrichment_tables_mut() {
            Some(ref mut table) => {
                table.add_index(&self.table, vec!["nork"]);
            }
            // We shouldn't reach this point since the type checker will ensure the table exists before this function is called.
            None => (),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().unknown()
    }
}
