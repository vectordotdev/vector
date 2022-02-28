use url::form_urlencoded::Parse;
use vrl::prelude::*;

/// Inner kind for a parsed query string.
pub(super) fn query_inner_kind() -> Collection<Field> {
    Collection::from_unknown(Kind::bytes().or_array(Collection::any()))
}

/// Parse a query string into a map of String -> Value.
pub(super) fn parse_query(query: Parse) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::new();

    for (k, value) in query {
        let value = value.as_ref();
        result
            .entry(k.into_owned())
            .and_modify(|v| {
                match v {
                    Value::Array(v) => {
                        v.push(value.into());
                    }
                    v => {
                        *v = Value::Array(vec![v.to_owned(), value.into()]);
                    }
                };
            })
            .or_insert_with(|| value.into());
    }

    result
}
