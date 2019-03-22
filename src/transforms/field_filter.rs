use super::Transform;
use crate::record::Record;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FieldFilterConfig {
    pub field: String,
    pub value: String,
}

#[typetag::serde(name = "field_filter")]
impl crate::topology::config::TransformConfig for FieldFilterConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(FieldFilter::new(
            self.field.clone(),
            self.value.clone(),
        )))
    }
}

pub struct FieldFilter {
    field_name: Atom,
    value: String,
}

impl FieldFilter {
    pub fn new(field_name: String, value: String) -> Self {
        Self {
            field_name: field_name.into(),
            value,
        }
    }
}

impl Transform for FieldFilter {
    fn transform(&self, record: Record) -> Option<Record> {
        if record.custom.get(&self.field_name) == Some(&self.value) {
            Some(record)
        } else {
            None
        }
    }
}
