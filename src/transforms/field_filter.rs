use super::Transform;
use crate::{
    topology::config::{DataType, TransformConfig},
    Event,
};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FieldFilterConfig {
    pub field: String,
    pub value: String,
}

#[typetag::serde(name = "field_filter")]
impl TransformConfig for FieldFilterConfig {
    fn build(&self) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(FieldFilter::new(
            self.field.clone(),
            self.value.clone(),
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "field_filter"
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
    fn transform(&mut self, event: Event) -> Option<Event> {
        if event
            .as_log()
            .get(&self.field_name)
            .map(|f| f.as_bytes())
            .map_or(false, |b| b == self.value.as_bytes())
        {
            Some(event)
        } else {
            None
        }
    }
}
