use super::Transform;
use crate::record::Record;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AddFieldsConfig {
    pub fields: IndexMap<String, String>,
}

pub struct AddFields {
    fields: IndexMap<Atom, String>,
}

#[typetag::serde(name = "augmenter")]
impl crate::topology::config::TransformConfig for AddFieldsConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(AddFields::new(self.fields.clone())))
    }
}

impl AddFields {
    pub fn new(fields: IndexMap<String, String>) -> Self {
        let fields = fields.into_iter().map(|(k, v)| (k.into(), v)).collect();

        AddFields { fields }
    }
}

impl Transform for AddFields {
    fn transform(&self, mut record: Record) -> Option<Record> {
        for (key, value) in self.fields.clone() {
            record.insert_explicit(key, value.into());
        }

        Some(record)
    }
}

#[cfg(test)]
mod tests {
    use super::AddFields;
    use crate::{record::Record, transforms::Transform};
    use indexmap::IndexMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn add_fields_record() {
        let record = Record::from("augment me");
        let mut fields = IndexMap::new();
        fields.insert("some_key".into(), "some_val".into());
        let augment = AddFields::new(fields);

        let new_record = augment.transform(record).unwrap();

        let key = Atom::from("some_key".to_string());
        let kv = new_record.get(&key);

        let val = "some_val".to_string();
        assert_eq!(kv, Some(&val.into()));
    }
}
