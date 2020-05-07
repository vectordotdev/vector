use super::Transform;
use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    wasm::{WasmModule, WasmModuleConfig},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use vector_wasm::Role;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct WasmConfig {
    pub module: PathBuf,
}

impl Into<WasmModuleConfig> for WasmConfig {
    fn into(self) -> WasmModuleConfig {
        WasmModuleConfig::new(Role::Transform, self.module)
    }
}

inventory::submit! {
    TransformDescription::new_without_default::<WasmConfig>("wasm")
}

#[typetag::serde(name = "wasm")]
impl TransformConfig for WasmConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(Wasm::new(self.clone())?))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "wasm"
    }
}

#[derive(Debug)]
pub struct Wasm {
    module: WasmModule,
}

impl Wasm {
    pub fn new(config: WasmConfig) -> crate::Result<Self> {
        let module = WasmModule::build(config)?;

        Ok(Self { module })
    }
}

impl Transform for Wasm {
    fn transform(&mut self, event: Event) -> Option<Event> {
        self.module
            .process(event)
            .map(|outputs| outputs.into_iter().next())
            .unwrap_or(None)
    }
}

#[cfg(test)]
mod tests {
    use super::Wasm;
    use crate::{event::Event, transforms::Transform};
    use serde_json::Value;
    use std::{collections::HashMap, fs, io::Read, path::Path};

    fn parse_config(s: &str) -> crate::Result<Wasm> {
        Wasm::new(toml::from_str(s).unwrap())
    }

    fn parse_event_artifact(path: impl AsRef<Path>) -> crate::Result<Event> {
        let mut event = Event::new_empty_log();
        let mut test_file = fs::File::open(path)?;

        let mut buf = String::new();
        test_file.read_to_string(&mut buf)?;
        let test_json: HashMap<String, Value> = serde_json::from_str(&buf)?;

        for (key, value) in test_json {
            event.as_mut_log().insert(key, value.clone());
        }
        Ok(event)
    }

    #[test]
    fn poc() -> crate::Result<()> {
        use serde_json::json;
        let mut transform = parse_config(
            r#"
            module = "tests/data/wasm/protobuf/protobuf.wat"
            "#,
        )?;

        let input = parse_event_artifact("tests/data/wasm/protobuf/demo.json")?;

        let mut expected = input.clone();
        expected.as_mut_log().insert(
            "processed",
            json!({
                "people": [
                    {
                        "name": "Foo",
                        "id": 1,
                        "email": "foo@test.com",
                        "phones": [],
                    }
                ]
            }),
        );

        let new_event = transform.transform(input);

        assert_eq!(new_event, Some(expected));
        Ok(())
    }
}
