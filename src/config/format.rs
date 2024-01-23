//! Support for loading configs from multiple formats.

#![deny(missing_docs, missing_debug_implementations)]

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::de;

/// A type alias to better capture the semantics.
pub type FormatHint = Option<Format>;

/// The format used to represent the configuration data.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Format {
    /// TOML format is used.
    #[default]
    Toml,
    /// JSON format is used.
    Json,
    /// YAML format is used.
    Yaml,
}

impl FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "toml" => Ok(Format::Toml),
            "yaml" => Ok(Format::Yaml),
            "json" => Ok(Format::Json),
            _ => Err(format!("Invalid format: {}", s)),
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let format = match self {
            Format::Toml => "toml",
            Format::Json => "json",
            Format::Yaml => "yaml",
        };
        write!(f, "{}", format)
    }
}

impl Format {
    /// Obtain the format from the file path using extension as a hint.
    pub fn from_path<T: AsRef<Path>>(path: T) -> Result<Self, T> {
        match path.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some("toml") => Ok(Format::Toml),
            Some("yaml") | Some("yml") => Ok(Format::Yaml),
            Some("json") => Ok(Format::Json),
            _ => Err(path),
        }
    }
}

/// Parse the string represented in the specified format.
pub fn deserialize<T>(content: &str, format: Format) -> Result<T, Vec<String>>
where
    T: de::DeserializeOwned,
{
    match format {
        Format::Toml => toml::from_str(content).map_err(|e| vec![e.to_string()]),
        Format::Yaml => serde_yaml::from_str(content).map_err(|e| vec![e.to_string()]),
        Format::Json => serde_json::from_str(content).map_err(|e| vec![e.to_string()]),
    }
}

/// Serialize the specified `value` into a string.
pub fn serialize<T>(value: &T, format: Format) -> Result<String, String>
where
    T: serde::ser::Serialize,
{
    match format {
        Format::Toml => toml::to_string(value).map_err(|e| e.to_string()),
        Format::Yaml => serde_yaml::to_string(value).map_err(|e| e.to_string()),
        Format::Json => serde_json::to_string_pretty(value).map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test ensures the logic to guess file format from the file path
    /// works correctly.
    /// Like all other tests, it also demonstrates various cases and how our
    /// code behaves when it encounters them.
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
            ("config.toml", Some(Format::Toml)),
            ("/config.toml", Some(Format::Toml)),
            ("/dir/config.toml", Some(Format::Toml)),
            ("config.qq.toml", Some(Format::Toml)),
            // YAML
            ("config.yaml", Some(Format::Yaml)),
            ("/config.yaml", Some(Format::Yaml)),
            ("/dir/config.yaml", Some(Format::Yaml)),
            ("config.qq.yaml", Some(Format::Yaml)),
            ("config.yml", Some(Format::Yaml)),
            ("/config.yml", Some(Format::Yaml)),
            ("/dir/config.yml", Some(Format::Yaml)),
            ("config.qq.yml", Some(Format::Yaml)),
            // JSON
            ("config.json", Some(Format::Json)),
            ("/config.json", Some(Format::Json)),
            ("/dir/config.json", Some(Format::Json)),
            ("config.qq.json", Some(Format::Json)),
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
            [enrichment_tables.csv]
            type = "file"
            file.path = "/tmp/file.csv"
            file.encoding.type = "csv"
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
            encoding.codec = "text"
            address = "127.0.0.1:9999"
        "#;

        let cases = vec![
            // Valid empty inputs should resolve to an empty, default value.
            ("", Format::Toml, Ok("")),
            ("{}", Format::Yaml, Ok("")),
            ("{}", Format::Json, Ok("")),
            ("", Format::Yaml, Ok("")),
            // Invalid "empty" inputs should resolve to an error.
            (
                "",
                Format::Json,
                Err(vec!["EOF while parsing a value at line 1 column 0"]),
            ),
            // Sample config.
            (SAMPLE_TOML, Format::Toml, Ok(SAMPLE_TOML)),
            (
                // YAML is sensitive to leading whitespace and linebreaks.
                concat_with_newlines!(
                    r#"enrichment_tables:"#,
                    r#"  csv:"#,
                    r#"    type: "file""#,
                    r#"    file:"#,
                    r#"      path: "/tmp/file.csv""#,
                    r#"      encoding:"#,
                    r#"        type: "csv""#,
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
                    r#"    encoding:"#,
                    r#"      codec: "text""#,
                    r#"    address: "127.0.0.1:9999""#,
                ),
                Format::Yaml,
                Ok(SAMPLE_TOML),
            ),
            (
                r#"
                {
                    "enrichment_tables": {
                        "csv": {
                            "type": "file",
                            "file": {
                              "path": "/tmp/file.csv",
                              "encoding": {
                                "type": "csv"
                              }
                            }
                        }
                    },
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
                            "encoding": {
                                "codec": "text"
                            },
                            "address": "127.0.0.1:9999"
                        }
                    }
                }
                "#,
                Format::Json,
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
                    let expected_output: ConfigBuilder = deserialize(expected, Format::Toml)
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
