use crate::{
    config::{DataType, TransformConfig, TransformDescription},
    event::Event,
    internal_events::{RemapEventProcessed, RemapFailedMapping},
    transforms::{FunctionTransform, Transform},
    Result,
};
use remap::{Program, Runtime};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct RemapConfig {
    pub source: String,
    pub drop_on_err: bool,
}

inventory::submit! {
    TransformDescription::new::<RemapConfig>("remap")
}

impl_generate_config_from_default!(RemapConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "remap")]
impl TransformConfig for RemapConfig {
    async fn build(&self) -> Result<Transform> {
        Remap::new(self.clone()).map(Transform::function)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "remap"
    }
}

#[derive(Debug, Clone)]
pub struct Remap {
    program: Program,
    drop_on_err: bool,
}

impl Remap {
    pub fn new(config: RemapConfig) -> crate::Result<Remap> {
        Ok(Remap {
            program: Program::new(&config.source, &crate::remap::FUNCTIONS_MUT)?,
            drop_on_err: config.drop_on_err,
        })
    }
}

impl FunctionTransform for Remap {
    fn transform(&self, output: &mut Vec<Event>, mut event: Event) {
        emit!(RemapEventProcessed);

        let mut runtime = Runtime::default();

        if let Err(error) = runtime.execute(&mut event, &self.program) {
            emit!(RemapFailedMapping {
                event_dropped: self.drop_on_err,
                error: error.to_string(),
            });

            if self.drop_on_err {
                return;
            }
        }

        output.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemapConfig>();
    }

    fn get_field_string(event: &Event, field: &str) -> String {
        event.as_log().get(field).unwrap().to_string_lossy()
    }

    #[test]
    fn check_remap_adds() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("copy_from", "buz");
            event
        };

        let conf = RemapConfig {
            source: r#"  .foo = "bar"
  .bar = "baz"
  .copy = .copy_from
"#
            .to_string(),
            drop_on_err: true,
        };
        let tform = Remap::new(conf).unwrap();

        let result = tform.transform_one(event).unwrap();
        assert_eq!(get_field_string(&result, "message"), "augment me");
        assert_eq!(get_field_string(&result, "copy_from"), "buz");
        assert_eq!(get_field_string(&result, "foo"), "bar");
        assert_eq!(get_field_string(&result, "bar"), "baz");
        assert_eq!(get_field_string(&result, "copy"), "buz");
    }
}
