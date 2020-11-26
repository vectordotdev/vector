//! Support for loading configs from multiple formats.

#![deny(missing_docs, missing_debug_implementations)]

use serde::de;
use std::path::Path;

/// The format used to represent the configuration data.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Format {
    /// The format could not be determined.
    Unknown,
    /// TOML format is used.
    TOML,
    /// JSON format is used.
    JSON,
    /// YAML format is used.
    YAML,
}

impl Default for Format {
    fn default() -> Self {
        Format::TOML
    }
}

impl Format {
    /// Returns the format as is unless it's unknown.
    /// If the format is unknown - executes the function and returns the result
    /// of the function.
    pub fn known_or<F>(self, f: F) -> Self
    where
        F: FnOnce() -> Self,
    {
        match self {
            Format::Unknown => f(),
            _ => self,
        }
    }
}

impl<T> From<T> for Format
where
    T: AsRef<Path>,
{
    /// Obtain the format from the file path using extension as a hint.
    fn from(path: T) -> Self {
        match path.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some("toml") => Format::TOML,
            Some("yaml") | Some("yml") => Format::YAML,
            Some("json") => Format::JSON,
            _ => Format::Unknown,
        }
    }
}

/// Parse the string represented in the specified format.
/// If the format is unknown - fallback to the default format and attempt
/// parsing using that.
pub fn deserialize<T>(content: &str, format: Format) -> Result<T, Vec<String>>
where
    T: de::DeserializeOwned,
{
    match format.known_or(Format::default) {
        Format::Unknown => unreachable!(),
        Format::TOML => toml::from_str(content).map_err(|e| vec![e.to_string()]),
        Format::YAML => serde_yaml::from_str(content).map_err(|e| vec![e.to_string()]),
        Format::JSON => serde_json::from_str(content).map_err(|e| vec![e.to_string()]),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::ConfigBuilder;

    use super::*;

    /// This test ensures the logic to guess file format from the file path
    /// works correctly.
    /// Like all other tests, it also demonstrates various cases and how our
    /// code behaves when it enounters them.
    #[test]
    fn test_from_path() {
        let cases = vec![
            // Unknown - odd variants.
            ("", Format::Unknown),
            (".", Format::Unknown),
            // Unknown - no ext.
            ("myfile", Format::Unknown),
            ("mydir/myfile", Format::Unknown),
            ("/mydir/myfile", Format::Unknown),
            // Unknown - some unknown ext.
            ("myfile.myext", Format::Unknown),
            ("mydir/myfile.myext", Format::Unknown),
            ("/mydir/myfile.myext", Format::Unknown),
            // Unknown - some unknown ext after known ext.
            ("myfile.toml.myext", Format::Unknown),
            ("myfile.yaml.myext", Format::Unknown),
            ("myfile.yml.myext", Format::Unknown),
            ("myfile.json.myext", Format::Unknown),
            // Unknown - invalid case.
            ("myfile.TOML", Format::Unknown),
            ("myfile.YAML", Format::Unknown),
            ("myfile.YML", Format::Unknown),
            ("myfile.JSON", Format::Unknown),
            // Unknown - nothing but extension.
            (".toml", Format::Unknown),
            (".yaml", Format::Unknown),
            (".yml", Format::Unknown),
            (".json", Format::Unknown),
            // TOML
            ("config.toml", Format::TOML),
            ("/config.toml", Format::TOML),
            ("/dir/config.toml", Format::TOML),
            ("config.qq.toml", Format::TOML),
            // YAML
            ("config.yaml", Format::YAML),
            ("/config.yaml", Format::YAML),
            ("/dir/config.yaml", Format::YAML),
            ("config.qq.yaml", Format::YAML),
            ("config.yml", Format::YAML),
            ("/config.yml", Format::YAML),
            ("/dir/config.yml", Format::YAML),
            ("config.qq.yml", Format::YAML),
            // JSON
            ("config.json", Format::JSON),
            ("/config.json", Format::JSON),
            ("/dir/config.json", Format::JSON),
            ("config.qq.json", Format::JSON),
        ];

        for (input, expected) in cases {
            let output = Format::from(std::path::PathBuf::from(input));
            assert_eq!(expected, output, "{}", input)
        }
    }

    macro_rules! concat_with_newlines {
        ($($e:expr,)*) => { concat!( $($e, "\n"),+ ) };
    }

    // Here we test that the deserializations from various formats match
    // the TOML format.
    #[test]
    fn test_deserialize_matches_toml() {
        const SAMPLE_TOML: &str = r#"
            [sources.in]
            type = "socket"
            mode = "tcp"
            address = "127.0.0.1:1235"
            [transforms.sampler]
            type = "sampler"
            inputs = ["in"]
            rate = 10
            [sinks.out]
            type = "socket"
            mode = "tcp"
            inputs = ["sampler"]
            encoding = "text"
            address = "127.0.0.1:9999"
        "#;

        let cases = vec![
            // Valid empty inputs should resolve to default.
            ("", Format::Unknown, Ok("")),
            ("", Format::TOML, Ok("")),
            ("{}", Format::YAML, Ok("")),
            ("{}", Format::JSON, Ok("")),
            // Invalid "empty" inputs should resolve to an error.
            ("", Format::YAML, Err(vec!["EOF while parsing a value"])),
            (
                "",
                Format::JSON,
                Err(vec!["EOF while parsing a value at line 1 column 0"]),
            ),
            // Sample config.
            (SAMPLE_TOML, Format::Unknown, Ok(SAMPLE_TOML)),
            (SAMPLE_TOML, Format::TOML, Ok(SAMPLE_TOML)),
            (
                // YAML is sensitive to leading whitespace and linebreaks.
                concat_with_newlines!(
                    r#"sources:"#,
                    r#"  in:"#,
                    r#"    type: "socket""#,
                    r#"    mode: "tcp""#,
                    r#"    address: "127.0.0.1:1235""#,
                    r#"transforms:"#,
                    r#"  sampler:"#,
                    r#"    type: "sampler""#,
                    r#"    inputs: ["in"]"#,
                    r#"    rate: 10"#,
                    r#"sinks:"#,
                    r#"  out:"#,
                    r#"    type: "socket""#,
                    r#"    mode: "tcp""#,
                    r#"    inputs: ["sampler"]"#,
                    r#"    encoding: "text""#,
                    r#"    address: "127.0.0.1:9999""#,
                ),
                Format::YAML,
                Ok(SAMPLE_TOML),
            ),
            (
                r#"
                {
                    "sources": {
                        "in": {
                            "type": "socket",
                            "mode": "tcp",
                            "address": "127.0.0.1:1235"
                        }
                    },
                    "transforms": {
                        "sampler": {
                            "type": "sampler",
                            "inputs": ["in"],
                            "rate": 10
                        }
                    },
                    "sinks": {
                        "out": {
                            "type": "socket",
                            "mode": "tcp",
                            "inputs": ["sampler"],
                            "encoding": "text",
                            "address": "127.0.0.1:9999"
                        }
                    }
                }
                "#,
                Format::JSON,
                Ok(SAMPLE_TOML),
            ),
        ];

        for (input, format, expected) in cases {
            // Here we use the same trick as at ConfigBuilder::clone impl to
            // compare the results.

            let output = deserialize(input, format);
            match expected {
                Ok(expected) => {
                    #[allow(clippy::expect_fun_call)] // false positive
                    let output: ConfigBuilder = output.expect(&format!(
                        "expected Ok, got Err with format {:?} and input {:?}",
                        format, input
                    ));
                    let output_json = serde_json::to_value(output).unwrap();
                    let expected_output: ConfigBuilder = deserialize(expected, Format::TOML)
                        .expect("Invalid TOML passed as an expectation");
                    let expected_json = serde_json::to_value(expected_output).unwrap();
                    assert_eq!(expected_json, output_json, "{}", input)
                }
                Err(expected) => assert_eq!(
                    expected,
                    output.expect_err(&format!(
                        "expected Err, got Ok with format {:?} and input {:?}",
                        format, input
                    )),
                    "{}",
                    input
                ),
            }
        }
    }
}
