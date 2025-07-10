use super::all_fields;
use crate::event::{KeyString, ObjectMap};

/// Iterates over all paths in form `a.b[0].c[1]` in alphabetical order.
/// It is implemented as a wrapper around `all_fields` to reduce code
/// duplication.
pub fn keys(fields: &ObjectMap) -> impl Iterator<Item = KeyString> + '_ {
    all_fields(fields).map(|(k, _)| k)
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::{super::test::fields_from_json, *};

    #[test]
    fn keys_simple() {
        let fields = fields_from_json(json!({
            "field2": 3,
            "field1": 4,
            "field3": 5
        }));
        let expected: Vec<_> = vec!["field1", "field2", "field3"]
            .into_iter()
            .map(KeyString::from)
            .collect();

        let collected: Vec<_> = keys(&fields).collect();
        assert_eq!(collected, expected);
    }

    #[test]
    fn keys_nested() {
        let fields = fields_from_json(json!({
            "a": {
                "b": {
                    "c": 5
                },
                "a": 4,
                "array": [null, 3, {
                    "x": 1
                }, [2]]
            }
        }));
        let expected: Vec<_> = vec![
            "a.a",
            "a.array[0]",
            "a.array[1]",
            "a.array[2].x",
            "a.array[3][0]",
            "a.b.c",
        ]
        .into_iter()
        .map(KeyString::from)
        .collect();

        let collected: Vec<_> = keys(&fields).collect();
        assert_eq!(collected, expected);
    }
}
