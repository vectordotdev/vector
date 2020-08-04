use super::Transform;
use crate::{
    event::Event,
    internal_events::{RemapEventProcessed, RemapFailedMapping},
    mapping::{parser::parse as parse_mapping, Mapping},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct RemapConfig {
    pub mapping: String,
    pub drop_on_err: bool,
}

inventory::submit! {
    TransformDescription::new::<RemapConfig>("remap")
}

#[typetag::serde(name = "remap")]
impl TransformConfig for RemapConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(Remap::new(self.clone())?))
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

#[derive(Debug)]
pub struct Remap {
    mapping: Mapping,
    drop_on_err: bool,
}

impl Remap {
    pub fn new(config: RemapConfig) -> crate::Result<Remap> {
        Ok(Remap {
            mapping: parse_mapping(&config.mapping)?,
            drop_on_err: config.drop_on_err,
        })
    }
}

impl Transform for Remap {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        emit!(RemapEventProcessed);

        if let Err(err) = self.mapping.execute(&mut event) {
            error!(message = "mapping failed", %err);
            emit!(RemapFailedMapping { error: err });

            if self.drop_on_err {
                return None;
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn check_remap_adds() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("copy_from", "buz");
            event
        };

        let conf = RemapConfig {
            mapping: r#".foo = "bar"
            .bar = "baz"
            .copy = .copy_from"#
                .to_string(),
            drop_on_err: true,
        };
        let mut tform = Remap::new(conf).unwrap();

        let result = tform.transform(event.clone()).unwrap();
        assert_eq!(
            result
                .as_log()
                .get(&Atom::from("message"))
                .unwrap()
                .to_string_lossy(),
            "augment me"
        );
        assert_eq!(
            result
                .as_log()
                .get(&Atom::from("copy_from"))
                .unwrap()
                .to_string_lossy(),
            "buz"
        );
        assert_eq!(
            result
                .as_log()
                .get(&Atom::from("foo"))
                .unwrap()
                .to_string_lossy(),
            "bar"
        );
        assert_eq!(
            result
                .as_log()
                .get(&Atom::from("bar"))
                .unwrap()
                .to_string_lossy(),
            "baz"
        );
        assert_eq!(
            result
                .as_log()
                .get(&Atom::from("copy"))
                .unwrap()
                .to_string_lossy(),
            "buz"
        );
    }
}
