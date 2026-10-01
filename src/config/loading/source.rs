use std::io::Read;

use super::{
    ComponentHint, Loader, Process,
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
}

impl Default for SourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl Process for SourceLoader {
    /// Prepares input by simply reading bytes to a string. Unlike other loaders, there's no
    /// interpolation of environment variables. This is on purpose to preserve the original config.
    fn prepare<R: Read>(&mut self, mut input: R) -> Result<String, Vec<String>> {
        let mut source_string = String::new();
        input
            .read_to_string(&mut source_string)
            .map_err(|e| vec![e.to_string()])?;

        Ok(source_string)
    }

    /// Merge values by combining with the internal configuration map.
    fn merge(&mut self, map: ConfigMap, _hint: Option<ComponentHint>) -> Result<(), Vec<String>> {
        merge_into_map(&mut self.map, map)
    }
}

impl Loader<ConfigMap> for SourceLoader {
    /// Returns the resulting configuration map.
    fn take(self) -> ConfigMap {
        self.map
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::SourceLoader;
    use crate::config::{
        Format,
        loading::{loader_from_input, representation::ConfigMap},
    };

    #[test]
    fn preserves_explicit_json_and_yaml_null() {
        for (input, format) in [
            (r#"{"optional": null}"#, Format::Json),
            ("optional: null", Format::Yaml),
        ] {
            let map: ConfigMap =
                loader_from_input(SourceLoader::new(), input.as_bytes(), format).unwrap();

            assert_eq!(map.get("optional"), Some(&Value::Null));
            assert_eq!(Value::Object(map), json!({ "optional": null }));
        }
    }
}
