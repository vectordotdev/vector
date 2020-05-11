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

    fn parse_event_artifact(path: impl AsRef<Path>) -> crate::Result<Option<Event>> {
        let mut event = Event::new_empty_log();
        let mut test_file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => Err(e)?,
        };

        let mut buf = String::new();
        test_file.read_to_string(&mut buf)?;
        let test_json: HashMap<String, Value> = serde_json::from_str(&buf)?;

        for (key, value) in test_json {
            event.as_mut_log().insert(key, value.clone());
        }
        Ok(Some(event))
    }

    #[test]
    fn protobuf_happy() -> crate::Result<()> {
        let mut transform = parse_config(
            r#"
            module = "tests/data/wasm/protobuf/protobuf.wat"
            "#,
        )?;

        let input =
            parse_event_artifact("tests/data/wasm/protobuf/fixtures/happy/input.json")?.unwrap();

        let output = transform.transform(input);

        let expected =
            parse_event_artifact("tests/data/wasm/protobuf/fixtures/happy/expected.json")?;
        assert_eq!(output, expected);
        Ok(())
    }

    #[test]
    fn protobuf_sad() -> crate::Result<()> {
        let mut transform = parse_config(
            r#"
            module = "tests/data/wasm/protobuf/protobuf.wat"
            "#,
        )?;

        let input =
            parse_event_artifact("tests/data/wasm/protobuf/fixtures/sad/input.json")?.unwrap();

        let output = transform.transform(input);

        let expected = parse_event_artifact("tests/data/wasm/protobuf/fixtures/sad/expected.json")?;
        assert_eq!(output, expected);
        Ok(())
    }

    #[test]
    fn add_fields() -> crate::Result<()> {
        let mut transform = parse_config(
            r#"
            module = "tests/data/wasm/add_fields/add_fields.wat"
            "#,
        )?;

        let input =
            parse_event_artifact("tests/data/wasm/add_fields/fixtures/a/input.json")?.unwrap();

        let output = transform.transform(input);

        let expected = parse_event_artifact("tests/data/wasm/add_fields/fixtures/a/expected.json")?;
        assert_eq!(output, expected);
        Ok(())
    }

    #[test]
    fn drop() -> crate::Result<()> {
        let mut transform = parse_config(
            r#"
            module = "tests/data/wasm/drop/drop.wat"
            "#,
        )?;

        let input = parse_event_artifact("tests/data/wasm/drop/fixtures/a/input.json")?.unwrap();

        let output = transform.transform(input);

        let expected = parse_event_artifact("tests/data/wasm/drop/fixtures/a/expected.json")?;
        assert_eq!(output, expected);
        Ok(())
    }
}
