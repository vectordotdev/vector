use super::{
    loader::{ConfigScope, ParsedInputs},
    representation::{ConfigMap, merge_into_map},
};

pub struct SourceLoader {
    map: ConfigMap,
}

impl SourceLoader {
    pub fn new() -> Self {
        Self {
            map: ConfigMap::new(),
        }
    }

    pub fn load_from_paths(self, paths: &[super::ConfigPath]) -> Result<ConfigMap, Vec<String>> {
        self.load_prepared(&ParsedInputs::from_paths(paths))
    }

    pub fn load_from_input(
        self,
        input: impl std::io::Read,
        format: super::Format,
    ) -> Result<ConfigMap, Vec<String>> {
        self.load_prepared(&ParsedInputs::from_input(input, format))
    }

    fn load_prepared(mut self, inputs: &ParsedInputs) -> Result<ConfigMap, Vec<String>> {
        inputs.assemble(
            |_| {},
            |map, scope| {
                let map = if matches!(scope, ConfigScope::DirectoryRoot) {
                    let mut root = ConfigMap::new();
                    for value in map.into_values() {
                        if let serde_json::Value::Object(map) = value {
                            merge_into_map(&mut root, map)?;
                        }
                    }
                    root
                } else {
                    map
                };
                merge_into_map(&mut self.map, map)
            },
        )?;
        Ok(self.map)
    }
}

impl Default for SourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::SourceLoader;
    use crate::config::{Format, loading::representation::ConfigMap};

    #[test]
    fn preserves_explicit_json_and_yaml_null() {
        for (input, format) in [
            (r#"{"optional": null}"#, Format::Json),
            ("optional: null", Format::Yaml),
        ] {
            let map: ConfigMap = SourceLoader::new()
                .load_from_input(input.as_bytes(), format)
                .unwrap();

            assert_eq!(map.get("optional"), Some(&Value::Null));
            assert_eq!(Value::Object(map), json!({ "optional": null }));
        }
    }
}
