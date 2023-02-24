//! Utilities shared between both VRL functions.
use std::collections::BTreeMap;

use ::value::Value;
use vrl::{
    diagnostic::{Label, Span},
    prelude::*,
};

use crate::{Case, Condition, IndexHandle, TableRegistry};

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

impl DiagnosticMessage for Error {
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

/// Evaluates the condition object to search the enrichment tables with.
pub(crate) fn evaluate_condition(key: &str, value: Value) -> Result<Condition> {
    Ok(match value {
        Value::Object(map) if map.contains_key("from") && map.contains_key("to") => {
            Condition::BetweenDates {
                field: key,
                from: *map
                    .get("from")
                    .expect("should contain from")
                    .as_timestamp()
                    .ok_or("from in condition must be a timestamp")?,
                to: *map
                    .get("to")
                    .expect("should contain to")
                    .as_timestamp()
                    .ok_or("to in condition must be a timestamp")?,
            }
        }
        _ => Condition::Equals { field: key, value },
    })
}

/// Add an index for the given condition to the given enrichment table.
pub(crate) fn add_index(
    registry: &mut TableRegistry,
    tablename: &str,
    case: Case,
    condition: &BTreeMap<String, expression::Expr>,
) -> std::result::Result<IndexHandle, ExpressionError> {
    let fields = condition
        .iter()
        .filter_map(|(field, value)| match value {
            expression::Expr::Container(expression::Container {
                variant: expression::Variant::Object(map),
            }) if map.contains_key("from") && map.contains_key("to") => None,
            _ => Some(field.as_ref()),
        })
        .collect::<Vec<_>>();
    let index = registry.add_index(tablename, case, &fields)?;

    Ok(index)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::test_util;

    #[test]
    fn add_indexes() {
        let mut registry = test_util::get_table_registry();
        let conditions = BTreeMap::from([(
            "field".to_owned(),
            expression::Literal::from("value").into(),
        )]);
        let index = add_index(&mut registry, "dummy1", Case::Insensitive, &conditions).unwrap();

        assert_eq!(IndexHandle(0), index);
    }

    #[test]
    fn add_indexes_with_dates() {
        let indexes = Arc::new(Mutex::new(Vec::new()));
        let dummy = test_util::DummyEnrichmentTable::new_with_index(indexes.clone());

        let mut registry =
            test_util::get_table_registry_with_tables(vec![("dummy1".to_string(), dummy)]);

        let conditions = BTreeMap::from([
            ("field1".into(), (expression::Literal::from("value")).into()),
            (
                "field2".into(),
                (expression::Container::new(expression::Variant::Object(
                    BTreeMap::from([
                        (
                            "from".into(),
                            (expression::Literal::from(
                                Utc.ymd(2015, 5, 15)
                                    .and_hms_opt(0, 0, 0)
                                    .expect("invalid timestamp"),
                            ))
                            .into(),
                        ),
                        (
                            "to".into(),
                            (expression::Literal::from(
                                Utc.ymd(2015, 6, 15)
                                    .and_hms_opt(0, 0, 0)
                                    .expect("invalid timestamp"),
                            ))
                            .into(),
                        ),
                    ])
                    .into(),
                )))
                .into(),
            ),
        ]);

        let index = add_index(&mut registry, "dummy1", Case::Sensitive, &conditions).unwrap();

        assert_eq!(IndexHandle(0), index);

        // Ensure only the exact match has been added as an index.
        let indexes = indexes.lock().unwrap();
        assert_eq!(vec![vec!["field1".to_string()]], *indexes);
    }
}
