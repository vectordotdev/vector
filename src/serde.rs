use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

pub fn default_true() -> bool {
    true
}

pub fn default_false() -> bool {
    false
}

pub fn to_string(value: impl serde::Serialize) -> String {
    let value = serde_json::to_value(value).unwrap();
    value.as_str().unwrap().into()
}

/// Answers "Is it possible to skip serializing this value, because it's the
/// default?"
pub(crate) fn skip_serializing_if_default<E: Default + PartialEq>(e: &E) -> bool {
    e == &E::default()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum FieldsOrValue<V> {
    Fields(Fields<V>),
    Value(V),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Fields<V>(IndexMap<String, FieldsOrValue<V>>);

impl<V: 'static> Fields<V> {
    pub fn all_fields(self) -> impl Iterator<Item = (Atom, V)> {
        self.0
            .into_iter()
            .map(|(k, v)| -> Box<dyn Iterator<Item = (Atom, V)>> {
                match v {
                    // boxing is used as a way to avoid incompatible types of the match arms
                    FieldsOrValue::Value(v) => Box::new(std::iter::once((k.into(), v))),
                    FieldsOrValue::Fields(f) => Box::new(
                        f.all_fields()
                            .into_iter()
                            .map(move |(nested_k, v)| (format!("{}.{}", k, nested_k).into(), v)),
                    ),
                }
            })
            .flatten()
    }
}
