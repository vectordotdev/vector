//! Utilities shared between both VRL functions.
use std::collections::BTreeMap;

use crate::{Case, Condition, IndexHandle, TableRegistry};
use vrl::diagnostic::{Label, Span};
use vrl::prelude::*;

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
pub(crate) fn evaluate_condition(key: &str, value: Value) -> ExpressionResult<Condition> {
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
    condition: &BTreeMap<KeyString, expression::Expr>,
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

pub(crate) fn is_case_sensitive(
    arguments: &ArgumentList,
    state: &TypeState,
) -> Result<Case, function::Error> {
    Ok(arguments
        .optional_literal("case_sensitive", state)?
        .map(|value| {
            let case_sensitive = value
                .as_boolean()
                .expect("case_sensitive should be boolean"); // This will have been caught by the type checker.

            if case_sensitive {
                Case::Sensitive
            } else {
                Case::Insensitive
            }
        })
        .unwrap_or(Case::Sensitive))
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
        let conditions =
            BTreeMap::from([("field".into(), expression::Literal::from("value").into())]);
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
                                Utc.with_ymd_and_hms(2015, 5, 15, 0, 0, 0)
                                    .single()
                                    .expect("invalid timestamp"),
                            ))
                            .into(),
                        ),
                        (
                            "to".into(),
                            (expression::Literal::from(
                                Utc.with_ymd_and_hms(2015, 6, 15, 0, 0, 0)
                                    .single()
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
