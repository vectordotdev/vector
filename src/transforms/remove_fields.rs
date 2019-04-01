use super::Transform;
use crate::record::Record;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RemoveFieldsConfig {
    pub fields: Vec<Atom>,
}

pub struct RemoveFields {
    fields: Vec<Atom>,
}

#[typetag::serde(name = "remove_fields")]
impl crate::topology::config::TransformConfig for RemoveFieldsConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(RemoveFields::new(self.fields.clone())))
    }
}

impl RemoveFields {
    pub fn new(fields: Vec<Atom>) -> Self {
        RemoveFields { fields }
    }
}

impl Transform for RemoveFields {
    fn transform(&self, mut record: Record) -> Option<Record> {
        for field in &self.fields {
            record.structured.remove(field);
        }

        Some(record)
    }
}

#[cfg(test)]
mod tests {
    use super::RemoveFields;
    use crate::{record::Record, transforms::Transform};
    use bytes::Bytes;

    #[test]
    fn remove_fields() {
        let mut record = Record::from("message");
        record
            .structured
            .insert("to_remove".into(), "some value".into());
        record
            .structured
            .insert("to_keep".into(), "another value".into());

        let transform = RemoveFields::new(vec!["to_remove".into(), "unknown".into()]);

        let new_record = transform.transform(record).unwrap();

        assert!(!new_record.structured.contains_key(&"to_remove".into()));
        assert!(!new_record.structured.contains_key(&"unknown".into()));
        assert_eq!(
            new_record.structured[&"to_keep".into()],
            Bytes::from("another value")
        );
    }
}
