use std::collections::BTreeMap;
use vrl::prelude::*;

use crate::vrl_util::is_case_sensitive;
use crate::{
    vrl_util::{self, add_index, evaluate_condition},
    Case, Condition, IndexHandle, TableRegistry, TableSearch,
};

fn get_enrichment_table_record(
    select: Option<Value>,
    enrichment_tables: &TableSearch,
    table: &str,
    case_sensitive: Case,
    condition: &[Condition],
    index: Option<IndexHandle>,
) -> Resolved {
    let select = select
        .map(|array| match array {
            Value::Array(arr) => arr
                .iter()
                .map(|value| Ok(value.try_bytes_utf8_lossy()?.to_string()))
                .collect::<std::result::Result<Vec<_>, _>>(),
            value => Err(ValueError::Expected {
                got: value.kind(),
                expected: Kind::array(Collection::any()),
            }),
        })
        .transpose()?;
    let data = enrichment_tables.find_table_row(
        table,
        case_sensitive,
        condition,
        select.as_ref().map(|select| select.as_ref()),
        index,
    )?;

    Ok(Value::Object(data))
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
            Parameter {
                keyword: "select",
                kind: kind::ARRAY,
                required: false,
            },
            Parameter {
                keyword: "case_sensitive",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "find records",
            source: r#"get_enrichment_table_record!("test", {"id": 1})"#,
            result: Ok(r#"{"id": 1, "firstname": "Bob", "surname": "Smith"}"#),
        }]
    }

    fn compile(
        &self,
        state: &TypeState,
        ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let registry = ctx
            .get_external_context_mut::<TableRegistry>()
            .ok_or(Box::new(vrl_util::Error::TablesNotLoaded) as Box<dyn DiagnosticMessage>)?;

        let tables = registry
            .table_ids()
            .into_iter()
            .map(Value::from)
            .collect::<Vec<_>>();

        let table = arguments
            .required_enum("table", &tables, state)?
            .try_bytes_utf8_lossy()
            .expect("table is not valid utf8")
            .into_owned();
        let condition = arguments.required_object("condition")?;

        let select = arguments.optional("select");

        let case_sensitive = is_case_sensitive(&arguments, state)?;
        let index = Some(
            add_index(registry, &table, case_sensitive, &condition)
                .map_err(|err| Box::new(err) as Box<_>)?,
        );

        Ok(GetEnrichmentTableRecordFn {
            table,
            condition,
            index,
            select,
            case_sensitive,
            enrichment_tables: registry.as_readonly(),
        }
        .as_expr())
    }
}

#[derive(Debug, Clone)]
pub struct GetEnrichmentTableRecordFn {
    table: String,
    condition: BTreeMap<KeyString, expression::Expr>,
    index: Option<IndexHandle>,
    select: Option<Box<dyn Expression>>,
    case_sensitive: Case,
    enrichment_tables: TableSearch,
}

impl FunctionExpression for GetEnrichmentTableRecordFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let condition = self
            .condition
            .iter()
            .map(|(key, value)| {
                let value = value.resolve(ctx)?;
                evaluate_condition(key, value)
            })
            .collect::<ExpressionResult<Vec<Condition>>>()?;

        let select = self
            .select
            .as_ref()
            .map(|array| array.resolve(ctx))
            .transpose()?;

        let table = &self.table;
        let case_sensitive = self.case_sensitive;
        let index = self.index;
        let enrichment_tables = &self.enrichment_tables;

        get_enrichment_table_record(
            select,
            enrichment_tables,
            table,
            case_sensitive,
            &condition,
            index,
        )
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::object(Collection::any()).fallible()
    }
}

#[cfg(test)]
mod tests {
    use vrl::compiler::prelude::TimeZone;
    use vrl::compiler::state::RuntimeState;
    use vrl::compiler::TargetValue;
    use vrl::value;
    use vrl::value::Secrets;

    use super::*;
    use crate::test_util::get_table_registry;

    #[test]
    fn find_table_row() {
        let registry = get_table_registry();
        let func = GetEnrichmentTableRecordFn {
            table: "dummy1".to_string(),
            condition: BTreeMap::from([(
                "field".into(),
                expression::Literal::from("value").into(),
            )]),
            index: Some(IndexHandle(999)),
            select: None,
            case_sensitive: Case::Sensitive,
            enrichment_tables: registry.as_readonly(),
        };

        let tz = TimeZone::default();
        let object: Value = BTreeMap::new().into();
        let mut target = TargetValue {
            value: object,
            metadata: value!({}),
            secrets: Secrets::new(),
        };
        let mut runtime_state = RuntimeState::default();
        let mut ctx = Context::new(&mut target, &mut runtime_state, &tz);

        registry.finish_load();

        let got = func.resolve(&mut ctx);

        assert_eq!(Ok(value! ({ "field": "result" })), got);
    }
}
