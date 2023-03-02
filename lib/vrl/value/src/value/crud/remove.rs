use crate::value::crud::{get_matching_coalesce_key, ValueCollection};
use crate::Value;
use lookup::lookup_v2::BorrowedSegment;

pub fn remove<'a, T: ValueCollection>(
    value: &mut T,
    key: &T::BorrowedKey,
    mut path_iter: impl Iterator<Item = BorrowedSegment<'a>>,
    prune: bool,
) -> Option<(Value, bool)> {
    match (value.get_mut_value(key), path_iter.next()) {
        (_, None) => value.remove_value(key),
        (Some(Value::Object(map)), Some(BorrowedSegment::Field(field))) => {
            let (prev_value, empty) = remove(map, field.as_ref(), path_iter, prune)?;
            if prune && empty {
                value.remove_value(key);
            }
            Some(prev_value)
        }
        (Some(Value::Object(map)), Some(BorrowedSegment::CoalesceField(field))) => {
            let matched_key = get_matching_coalesce_key(field, map, &mut path_iter).ok()?;
            let (prev_value, empty) = remove(map, matched_key.as_ref(), path_iter, prune)?;
            if prune && empty {
                value.remove_value(key);
            }
            Some(prev_value)
        }
        (Some(Value::Array(array)), Some(BorrowedSegment::Index(index))) => {
            let (prev_value, empty) = remove(array, &index, path_iter, prune)?;
            if prune && empty {
                value.remove_value(key);
            }
            Some(prev_value)
        }
        _ => return None,
    }
    .map(|prev| (prev, value.is_empty_collection()))
}

#[cfg(test)]
mod test {
    use crate::Value;
    use serde_json::json;

    #[test]
    fn remove_coalesce() {
        let mut value = Value::from(json!({
            "field": 123,
            "field2": 321
        }));
        assert_eq!(value.remove("(foo|bar)", false), None);
        assert_eq!(value.remove("(foo|field2)", false), Some(Value::from(321)));
        assert_eq!(
            value.remove(".(field|field2)", false),
            Some(Value::from(123))
        );
        assert_eq!(value.remove("(field|field2)", false), None);
    }

    #[test]
    fn remove_coalesce_2() {
        let mut value = Value::from(json!({
            "x": true,
            "y": {"z": true}
        }));
        assert_eq!(value.remove("(x|y).z", false), None);
        assert_eq!(value.remove("(x|y).z", false), None);
    }

    #[test]
    fn array_remove_from_middle() {
        let mut value = Value::Array(vec![Value::Null, Value::Integer(3)]);
        assert_eq!(value.remove("[0]", false), Some(Value::Null));
        assert_eq!(value.remove("[0]", false), Some(Value::Integer(3)));
        assert_eq!(value.remove("[0]", false), None);
    }

    #[test]
    fn remove_simple() {
        let mut value = Value::from(json!({
            "field": 123
        }));
        assert_eq!(value.remove("field", false), Some(Value::Integer(123)));
        assert_eq!(value.remove("field", false), None);
    }

    #[test]
    fn remove_nested() {
        let mut value = Value::from(json!({
            "a": {
                "b": {
                    "c": 5
                },
                "d": 4,
                "array": [null, 3, {
                    "x": 1
                }, [2]]
            }
        }));
        let queries = [
            ("a.b.c", Some(Value::Integer(5)), None),
            ("a.d", Some(Value::Integer(4)), None),
            ("a.array[2].x", Some(Value::Integer(1)), None),
            ("a.array[3][0]", Some(Value::Integer(2)), None),
            ("a.array[3][1]", None, None),
            ("a.x", None, None),
            ("z", None, None),
            (".123", None, None),
        ];

        for (query, expected_first, expected_second) in &queries {
            assert_eq!(value.remove(*query, false), *expected_first, "{query}");
            assert_eq!(value.remove(*query, false), *expected_second, "{query}");
        }

        assert_eq!(
            value,
            Value::from(json!({
                "a": {
                    "b": {},
                    "array": [
                        null,
                        3,
                        {},
                        [],
                    ],
                },
            }))
        );

        value.remove(".", false);
        assert_eq!(value, Value::from(json!({})));
        value.remove(".", true);
        assert_eq!(value, Value::from(json!({})));
    }

    #[test]
    fn remove_prune() {
        let mut value = Value::from(json!({
            "a": {
                "b": {
                    "c": vec![5]
                },
                "d": 4,
            }
        }));

        assert_eq!(value.remove("a.d", true), Some(Value::Integer(4)));
        assert_eq!(
            value,
            Value::from(json!({
                "a": {
                    "b": {
                        "c": vec![5]
                    }
                }
            }))
        );

        assert_eq!(value.remove("a.b.c[0]", true), Some(Value::Integer(5)));
        assert_eq!(value, Value::from(json!({})));
    }
}
