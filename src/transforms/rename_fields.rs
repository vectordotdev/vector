use super::Transform;
use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use string_cache::DefaultAtom as Atom;
use toml::value::Value as TomlValue;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RenameFieldsConfig {
    pub fields: IndexMap<String, TomlValue>,
}

pub struct RenameFields {
    fields: IndexMap<Atom, Atom>,
}

inventory::submit! {
    TransformDescription::new_without_default::<RenameFieldsConfig>("rename_fields")
}

#[typetag::serde(name = "rename_fields")]
impl TransformConfig for RenameFieldsConfig {
    fn build(&self, _exec: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(RenameFields::new(self.fields.clone())?))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "rename_fields"
    }
}

impl RenameFields {
    pub fn new(fields: IndexMap<String, TomlValue>) -> crate::Result<Self> {
        Ok(RenameFields {
            fields: fields
                .into_iter()
                .map(|kv| flatten(kv, None))
                .collect::<crate::Result<_>>()?,
        })
    }
}

#[derive(Debug, Eq, PartialEq, Snafu)]
enum FlattenError {
    #[snafu(display(
        "The key {:?} cannot be flattened. Is it a plain string or a `a.b.c` style map?",
        key
    ))]
    CannotFlatten { key: String },
}

fn flatten(kv: (String, TomlValue), prequel: Option<String>) -> crate::Result<(Atom, Atom)> {
    let (k, v) = kv;
    match v {
        TomlValue::String(s) => match prequel {
            Some(prequel) => Ok((format!("{}.{}", prequel, k).into(), s.into())),
            None => Ok((k.into(), s.into())),
        },
        TomlValue::Table(map) => {
            if map.len() > 1 {
                Err(Box::new(FlattenError::CannotFlatten { key: k }))
            } else {
                let sub_kv = map.into_iter().next().expect("Map of len 1 has no values");
                let key = match prequel {
                    Some(prequel) => format!("{}.{}", prequel, k),
                    None => k,
                };
                flatten(sub_kv, Some(key))
            }
        }
        TomlValue::Integer(_)
        | TomlValue::Float(_)
        | TomlValue::Boolean(_)
        | TomlValue::Datetime(_)
        | TomlValue::Array(_) => Err(Box::new(FlattenError::CannotFlatten { key: k })),
    }
}

impl Transform for RenameFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        for (old_key, new_key) in &self.fields {
            let log = event.as_mut_log();
            if let Some(v) = log.remove(&old_key) {
                log.insert(new_key.clone(), v)
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::RenameFields;
    use crate::{event::Event, transforms::Transform};
    use indexmap::map::IndexMap;

    #[test]
    fn rename_fields() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("to_move", "some value");
        event.as_mut_log().insert("do_not_move", "not moved");
        let mut fields = IndexMap::new();
        fields.insert("to_move".into(), "moved".into());
        fields.insert("not_present".into(), "should_not_exist".into());

        let mut transform = RenameFields::new(fields).unwrap();

        let new_event = transform.transform(event).unwrap();

        assert!(new_event.as_log().get(&"to_move".into()).is_none());
        assert_eq!(new_event.as_log()[&"moved".into()], "some value".into());
        assert!(new_event.as_log().get(&"not_present".into()).is_none());
        assert!(new_event.as_log().get(&"should_not_exist".into()).is_none());
        assert_eq!(
            new_event.as_log()[&"do_not_move".into()],
            "not moved".into()
        );
    }
}
