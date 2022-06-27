use crate::value::crud::ValueCollection;
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
}
