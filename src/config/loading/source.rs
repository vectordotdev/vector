use super::{
    loader::{ConfigLoader, ConfigScope, LoadMode, ParsedInputs},
    representation::{ConfigMap, merge_into_map},
};

pub struct SourceLoader {
    map: ConfigMap,
}

impl SourceLoader {
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: ConfigMap::new(),
        }
    }

    /// Loads and merges configuration values from paths without interpolation.
    ///
    /// # Errors
    ///
    /// Returns errors encountered while reading or parsing input, or merging
    /// incompatible configuration values.
    pub fn load_from_paths(self, paths: &[super::ConfigPath]) -> Result<ConfigMap, Vec<String>> {
        self.load_prepared(&ParsedInputs::from_paths(paths))
    }

    /// Parses configuration values from a reader without interpolation.
    ///
    /// # Errors
    ///
    /// Returns errors encountered while reading or parsing input, or merging
    /// incompatible configuration values.
    pub fn load_from_input(
        self,
        input: impl std::io::Read,
        format: super::Format,
    ) -> Result<ConfigMap, Vec<String>> {
        self.load_prepared(&ParsedInputs::from_input(input, format))
    }

    fn load_prepared(mut self, inputs: &ParsedInputs) -> Result<ConfigMap, Vec<String>> {
        inputs.load_into(&mut self)?;
        Ok(self.map)
    }
}

impl ConfigLoader for SourceLoader {
    const MODE: LoadMode = LoadMode::Raw;

    fn merge(&mut self, map: ConfigMap, _scope: ConfigScope) -> Result<(), Vec<String>> {
        merge_into_map(&mut self.map, map)
    }
}

impl Default for SourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use serde_json::{Value, json};

    use super::SourceLoader;
    use crate::config::{ConfigPath, Format, loading::representation::ConfigMap};

    #[test]
    fn directory_roots_preserve_raw_merge_rules() {
        const FIRST: &str = indoc! {"
            proxy:
              enabled: true
        "};
        const SECOND: &str = indoc! {r"
            proxy:
              enabled: null
              http: http://proxy.example
        "};

        for enabled in [json!(true), json!("true")] {
            let directory = tempfile::tempdir().unwrap();
            std::fs::write(directory.path().join("first.yaml"), FIRST).unwrap();
            let mut second: Value = serde_yaml::from_str(SECOND).unwrap();
            second["proxy"]["enabled"] = enabled.clone();
            std::fs::write(
                directory.path().join("second.yaml"),
                serde_yaml::to_string(&second).unwrap(),
            )
            .unwrap();

            let result = SourceLoader::new()
                .load_from_paths(&[ConfigPath::Dir(directory.path().to_owned())]);
            if enabled.is_boolean() {
                assert_eq!(Value::Object(result.unwrap()), second);
            } else {
                // Raw loading must not coerce the string to match the boolean.
                assert!(result.is_err());
            }
        }
    }

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
