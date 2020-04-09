use super::Transform;
use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FieldFilterConfig {
    pub field: String,
    pub value: String,
}

inventory::submit! {
    TransformDescription::new_without_default::<FieldFilterConfig>("field_filter")
}

#[typetag::serde(name = "field_filter")]
impl TransformConfig for FieldFilterConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        warn!(
            message =
                r#"The "field_filter" transform is deprecated, use the "filter" transform instead"#
        );
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
