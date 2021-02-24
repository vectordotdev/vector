//! Support for loading configs from multiple formats.

#![deny(missing_docs, missing_debug_implementations)]

use serde::de;
use std::path::Path;

/// A type alias to better capture the semantics.
pub type FormatHint = Option<Format>;

/// The format used to represent the configuration data.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Format {
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
    /// Obtain the format from the file path using extension as a hint.
    pub fn from_path<T: AsRef<Path>>(path: T) -> Result<Self, T> {
        match path.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some("toml") => Ok(Format::TOML),
            Some("yaml") | Some("yml") => Ok(Format::YAML),
            Some("json") => Ok(Format::JSON),
            _ => Err(path),
        }
    }
}

/// Parse the string represented in the specified format.
/// If the format is unknown - fallback to the default format and attempt
/// parsing using that.
pub fn deserialize<T>(content: &str, format: FormatHint) -> Result<T, Vec<String>>
where
    T: de::DeserializeOwned,
{
    match format.unwrap_or_default() {
        Format::TOML => toml::from_str(content).map_err(|e| vec![e.to_string()]),
        Format::YAML => serde_yaml::from_str(content).map_err(|e| vec![e.to_string()]),
        Format::JSON => serde_json::from_str(content).map_err(|e| vec![e.to_string()]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test ensures the logic to guess file format from the file path
    /// works correctly.
    /// Like all other tests, it also demonstrates various cases and how our
    /// code behaves when it enounters them.
    #[test]
    fn test_from_path() {
        let cases = vec![
            // Unknown - odd variants.
            ("", None),
            (".", None),
            // Unknown - no ext.
            ("myfile", None),
            ("mydir/myfile", None),
            ("/mydir/myfile", None),
            // Unknown - some unknown ext.
            ("myfile.myext", None),
            ("mydir/myfile.myext", None),
            ("/mydir/myfile.myext", None),
            // Unknown - some unknown ext after known ext.
            ("myfile.toml.myext", None),
            ("myfile.yaml.myext", None),
            ("myfile.yml.myext", None),
            ("myfile.json.myext", None),
            // Unknown - invalid case.
            ("myfile.TOML", None),
            ("myfile.YAML", None),
            ("myfile.YML", None),
            ("myfile.JSON", None),
            // Unknown - nothing but extension.
            (".toml", None),
            (".yaml", None),
            (".yml", None),
            (".json", None),
            // TOML
            ("config.toml", Some(Format::TOML)),
            ("/config.toml", Some(Format::TOML)),
            ("/dir/config.toml", Some(Format::TOML)),
            ("config.qq.toml", Some(Format::TOML)),
            // YAML
            ("config.yaml", Some(Format::YAML)),
            ("/config.yaml", Some(Format::YAML)),
            ("/dir/config.yaml", Some(Format::YAML)),
            ("config.qq.yaml", Some(Format::YAML)),
            ("config.yml", Some(Format::YAML)),
            ("/config.yml", Some(Format::YAML)),
            ("/dir/config.yml", Some(Format::YAML)),
            ("config.qq.yml", Some(Format::YAML)),
            // JSON
            ("config.json", Some(Format::JSON)),
            ("/config.json", Some(Format::JSON)),
            ("/dir/config.json", Some(Format::JSON)),
            ("config.qq.json", Some(Format::JSON)),
        ];

        for (input, expected) in cases {
            let output = Format::from_path(std::path::PathBuf::from(input));
            assert_eq!(expected, output.ok(), "{}", input)
        }
    }

    // Here we test that the deserializations from various formats match
    // the TOML format.
    #[cfg(all(
        feature = "sources-socket",
        feature = "transforms-sample",
        feature = "sinks-socket"
    ))]
    #[test]
    fn test_deserialize_matches_toml() {
        use crate::config::ConfigBuilder;

        macro_rules! concat_with_newlines {
            ($($e:expr,)*) => { concat!( $($e, "\n"),+ ) };
        }

        const SAMPLE_TOML: &str = r#"
            [sources.in]
            type = "socket"
            mode = "tcp"
            address = "127.0.0.1:1235"
            [transforms.sample]
            type = "sample"
            inputs = ["in"]
            rate = 10
            [sinks.out]
            type = "socket"
            mode = "tcp"
            inputs = ["sample"]
            encoding = "text"
            address = "127.0.0.1:9999"
        "#;

        let cases = vec![
            // Valid empty inputs should resolve to default.
            ("", None, Ok("")),
            ("", Some(Format::TOML), Ok("")),
            ("{}", Some(Format::YAML), Ok("")),
            ("{}", Some(Format::JSON), Ok("")),
            // Invalid "empty" inputs should resolve to an error.
            (
                "",
                Some(Format::YAML),
                Err(vec!["EOF while parsing a value"]),
            ),
            (
                "",
                Some(Format::JSON),
                Err(vec!["EOF while parsing a value at line 1 column 0"]),
            ),
            // Sample config.
            (SAMPLE_TOML, None, Ok(SAMPLE_TOML)),
            (SAMPLE_TOML, Some(Format::TOML), Ok(SAMPLE_TOML)),
            (
                // YAML is sensitive to leading whitespace and linebreaks.
                concat_with_newlines!(
                    r#"sources:"#,
                    r#"  in:"#,
                    r#"    type: "socket""#,
                    r#"    mode: "tcp""#,
                    r#"    address: "127.0.0.1:1235""#,
                    r#"transforms:"#,
                    r#"  sample:"#,
                    r#"    type: "sample""#,
                    r#"    inputs: ["in"]"#,
                    r#"    rate: 10"#,
                    r#"sinks:"#,
                    r#"  out:"#,
                    r#"    type: "socket""#,
                    r#"    mode: "tcp""#,
                    r#"    inputs: ["sample"]"#,
                    r#"    encoding: "text""#,
                    r#"    address: "127.0.0.1:9999""#,
                ),
                Some(Format::YAML),
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
                        "sample": {
                            "type": "sample",
                            "inputs": ["in"],
                            "rate": 10
                        }
                    },
                    "sinks": {
                        "out": {
                            "type": "socket",
                            "mode": "tcp",
                            "inputs": ["sample"],
                            "encoding": "text",
                            "address": "127.0.0.1:9999"
                        }
                    }
                }
                "#,
                Some(Format::JSON),
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
                    let expected_output: ConfigBuilder = deserialize(expected, Some(Format::TOML))
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
