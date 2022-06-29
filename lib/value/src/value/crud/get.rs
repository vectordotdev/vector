use crate::value::crud::{get_matching_coalesce_key, ValueCollection};
use crate::Value;
use lookup::lookup_v2::BorrowedSegment;

pub fn get<'a>(
    mut value: &Value,
    mut path_iter: impl Iterator<Item = BorrowedSegment<'a>>,
) -> Option<&Value> {
    loop {
        match (path_iter.next(), value) {
            (None, _) => return Some(value),
            (Some(BorrowedSegment::Field(key)), Value::Object(map)) => {
                match map.get_value(key.as_ref()) {
                    None => return None,
                    Some(nested_value) => {
                        value = nested_value;
                    }
                }
            }
            (Some(BorrowedSegment::CoalesceField(key)), Value::Object(map)) => {
                let matched_key = get_matching_coalesce_key(key, map, &mut path_iter).ok()?;
                value = map
                    .get_value(matched_key.as_ref())
                    .expect("this was already checked to exist");
            }
            (Some(BorrowedSegment::Index(index)), Value::Array(array)) => {
                match array.get_value(&index) {
                    None => return None,
                    Some(nested_value) => {
                        value = nested_value;
                    }
                }
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_negative_index() {
        assert_eq!(
            Value::from(json!([0, 1, 2, 3])).get("[-1]").cloned(),
            Some(Value::from(3))
        );
        assert_eq!(Value::from(json!([0, 1, 2, 3])).get("[-5]").cloned(), None);
    }

    #[test]
    fn test_coalesce() {
        assert_eq!(
            Value::from(json!({"b": 2})).get("(a|b)").cloned(),
            Some(Value::from(2))
        );
        assert_eq!(
            Value::from(json!({"b": {"x": 5}})).get("(a|b).x").cloned(),
            Some(Value::from(5))
        );
        assert_eq!(
            Value::from(json!({"b": {"x": 5}}))
                .get("(a|b).(y|x)")
                .cloned(),
            Some(Value::from(5))
        );
        assert_eq!(
            Value::from(json!({"a": 1})).get("(a|b)").cloned(),
            Some(Value::from(1))
        );
        assert_eq!(Value::from(json!({})).get("(a|b|c)").cloned(), None);

        assert_eq!(
            Value::from(json!({"a": true})).get("(a|b|c)").cloned(),
            Some(Value::from(true))
        );
    }
}
