use super::{Transform};
use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    foreign_modules::{WasmModuleConfig, WasmModule},
};
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;
use lazy_static::lazy_static;
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct WasmConfig {
    pub module: PathBuf,
}

impl Into<WasmModuleConfig> for WasmConfig {
    fn into(self) -> WasmModuleConfig {
        WasmModuleConfig::new(self.module, "cache")
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
    module: WasmModule<foreign_modules::roles::Transform>,
}

impl Wasm {
    pub fn new(config: WasmConfig) -> crate::Result<Self> {
        let module = WasmModule::build(config)?;

        Ok(Self {
            module
        })
    }
}

impl Transform for Wasm {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        self.module.process(event.clone()).unwrap_or(None)
    }
}

#[cfg(test)]
mod tests {
    use super::{WasmConfig, Wasm};
    use crate::{event::Event, transforms::Transform, topology::config::TransformConfig, event::LogEvent};
    use indexmap::IndexMap;
    use std::collections::HashMap;
    use string_cache::DefaultAtom as Atom;
    use serde_json::{json, Value};
    use std::{fs, io::Read, path::Path};

    fn parse_config(s: &str) -> crate::Result<Wasm> {
        Wasm::new(toml::from_str(s).unwrap())
    }

    fn parse_event_artifact(path: impl AsRef<Path>) -> crate::Result<Event> {
        let mut event = Event::new_empty_log();
        let mut test_file = fs::File::open("test-data/foreign_modules/protobuf/demo.json")?;

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
        let mut transform = parse_config(
            r#"
            module = "target/wasm32-wasi/release/protobuf.wasm"
            "#,
        )?;

        let mut input = parse_event_artifact("test-data/foreign_modules/protobuf/demo.json")?;

        let mut expected = input.clone();
        expected.as_mut_log().insert("processed", "{\"people\":[{\"name\":\"Foo\",\"id\":1,\"email\":\"foo@test.com\",\"phones\":[]}]}");

        let new_event = transform.transform(input);

        assert_eq!(new_event, Some(expected));
        Ok(())
    }
}
