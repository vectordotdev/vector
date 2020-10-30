use super::Transform;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformDescription},
    event::Event,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FieldFilterConfig {
    pub field: String,
    pub value: String,
}

inventory::submit! {
    TransformDescription::new::<FieldFilterConfig>("field_filter")
}

impl GenerateConfig for FieldFilterConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            field: String::new(),
            value: String::new(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "field_filter")]
impl TransformConfig for FieldFilterConfig {
    async fn build(&self) -> crate::Result<Box<dyn Transform>> {
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
    field_name: String,
    value: String,
}

impl FieldFilter {
    pub fn new(field_name: String, value: String) -> Self {
        Self { field_name, value }
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

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::FieldFilterConfig>();
    }
}
