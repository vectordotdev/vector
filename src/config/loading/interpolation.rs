use std::{collections::HashMap, sync::LazyLock};

use regex::{Captures, Regex};
use serde_json::Value;

use super::representation::ConfigMap;

/// Interpolates environment variables in string leaves without changing keys or value types.
pub fn interpolate_config_map_with_env_vars(
    map: &ConfigMap,
    vars: &HashMap<String, String>,
) -> Result<ConfigMap, Vec<String>> {
    interpolate_config_map(map, vars, interpolate)
}

/// Applies a string interpolator recursively, collecting errors from all string leaves.
pub(super) fn interpolate_config_map(
    map: &ConfigMap,
    vars: &HashMap<String, String>,
    interpolate_fn: impl Fn(&str, &HashMap<String, String>) -> Result<String, Vec<String>>,
) -> Result<ConfigMap, Vec<String>> {
    fn visit(
        value: &mut Value,
        interpolate: &impl Fn(&str) -> Result<String, Vec<String>>,
        errors: &mut Vec<String>,
    ) {
        match value {
            Value::String(string) => match interpolate(string) {
                Ok(interpolated) => *string = interpolated,
                Err(mut failures) => errors.append(&mut failures),
            },
            Value::Array(values) => {
                for value in values {
                    visit(value, interpolate, errors);
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    visit(value, interpolate, errors);
                }
            }
            _ => {}
        }
    }

    let mut result = map.clone();
    let mut errors = Vec::new();
    for value in result.values_mut() {
        visit(value, &|string| interpolate_fn(string, vars), &mut errors);
    }
    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

// Environment variable names can have any characters from the Portable Character Set other
// than NUL.  However, for Vector's interpolation, we are closer to what a shell supports which
// is solely of uppercase letters, digits, and the '_' (that is, the `[:word:]` regex class).
// In addition to these characters, we allow `.` as this commonly appears in environment
// variable names when they come from a Java properties file.
//
// https://pubs.opengroup.org/onlinepubs/000095399/basedefs/xbd_chap08.html
pub static ENVIRONMENT_VARIABLE_INTERPOLATION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        \$\$|
        \$([[:word:].]+)|
        \$\{([[:word:].]+)(?:(:?-|:?\?)([^}]*))?\}",
    )
    .unwrap()
});

/// Result<interpolated config, errors>
pub fn interpolate(input: &str, vars: &HashMap<String, String>) -> Result<String, Vec<String>> {
    let mut errors = Vec::new();

    let interpolated = ENVIRONMENT_VARIABLE_INTERPOLATION_REGEX
        .replace_all(input, |caps: &Captures<'_>| {
            let flags = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
            let def_or_err = caps.get(4).map(|m| m.as_str()).unwrap_or_default();
            caps.get(1)
                .or_else(|| caps.get(2))
                .map(|m| m.as_str())
                .map(|name| {
                    // Parsing has already fixed the configuration's structure. Newlines and
                    // other syntax characters remain part of this string value.
                    let val = vars.get(name).map(String::as_str);

                    match flags {
                        ":-" => match val {
                            Some(v) if !v.is_empty() => v,
                            _ => def_or_err,
                        },
                        "-" => val.unwrap_or(def_or_err),
                        ":?" => match val {
                            Some(v) if !v.is_empty() => v,
                            _ => {
                                errors.push(format!(
                                    "Non-empty environment variable required in config. name = {name:?}, error = {def_or_err:?}",
                                ));
                                ""
                            },
                        }
                        "?" => val.unwrap_or_else(|| {
                            errors.push(format!(
                                "Missing environment variable required in config. name = {name:?}, error = {def_or_err:?}",
                            ));
                            ""
                        }),
                        _ => val.unwrap_or_else(|| {
                            errors.push(format!(
                                "Missing environment variable in config. name = {name:?}",
                            ));
                            ""
                        }),
                    }
                })
                .unwrap_or("$")
                .to_string()
        })
        .into_owned();

    if errors.is_empty() {
        Ok(interpolated)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::{interpolate, interpolate_config_map_with_env_vars};

    #[test]
    fn tree_interpolation_preserves_keys_and_types() {
        let input = serde_json::json!({
            "${UNSET}": ["${VALUE}", {"${UNSET}": "${NUMBER}"}],
            "typed": [42, true, null],
            "boolean": "${BOOLEAN}"
        });
        let vars = HashMap::from([
            ("VALUE".into(), "\"quoted\": [value] # comment".into()),
            ("NUMBER".into(), "42".into()),
            ("BOOLEAN".into(), "true".into()),
        ]);
        let result =
            interpolate_config_map_with_env_vars(input.as_object().unwrap(), &vars).unwrap();
        assert_eq!(
            serde_json::Value::Object(result),
            serde_json::json!({
                "${UNSET}": ["\"quoted\": [value] # comment", {"${UNSET}": "42"}],
                "typed": [42, true, null],
                "boolean": "true"
            })
        );
        assert_eq!(input["boolean"], "${BOOLEAN}");
    }

    #[test]
    fn tree_interpolation_collects_errors_without_mutating_input() {
        let input = serde_json::json!({"values": ["${FIRST}", {"nested": "${SECOND}"}]});
        let original = input.clone();
        let errors =
            interpolate_config_map_with_env_vars(input.as_object().unwrap(), &HashMap::new())
                .unwrap_err();
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().any(|error| error.contains("FIRST")));
        assert!(errors.iter().any(|error| error.contains("SECOND")));
        assert_eq!(input, original);
    }

    #[test]
    fn interpolation() {
        let vars = vec![
            ("FOO".into(), "dogs".into()),
            ("FOOBAR".into(), "cats".into()),
            // Java commonly uses .s in env var names
            ("FOO.BAR".into(), "turtles".into()),
            ("EMPTY".into(), "".into()),
        ]
        .into_iter()
        .collect();

        assert_eq!("dogs", interpolate("$FOO", &vars).unwrap());
        assert_eq!("dogs", interpolate("${FOO}", &vars).unwrap());
        assert_eq!("cats", interpolate("${FOOBAR}", &vars).unwrap());
        assert_eq!("xcatsy", interpolate("x${FOOBAR}y", &vars).unwrap());
        assert!(interpolate("x$FOOBARy", &vars).is_err());
        assert_eq!("$ x", interpolate("$ x", &vars).unwrap());
        assert_eq!("$FOO", interpolate("$$FOO", &vars).unwrap());
        assert_eq!("dogs=bar", interpolate("$FOO=bar", &vars).unwrap());
        assert!(interpolate("$NOT_FOO", &vars).is_err());
        assert!(interpolate("$NOT-FOO", &vars).is_err());
        assert_eq!("turtles", interpolate("$FOO.BAR", &vars).unwrap());
        assert_eq!("${FOO x", interpolate("${FOO x", &vars).unwrap());
        assert_eq!("${}", interpolate("${}", &vars).unwrap());
        assert_eq!("dogs", interpolate("${FOO:-cats}", &vars).unwrap());
        assert_eq!("dogcats", interpolate("${NOT:-dogcats}", &vars).unwrap());
        assert_eq!(
            "dogs and cats",
            interpolate("${NOT:-dogs and cats}", &vars).unwrap()
        );
        assert_eq!("${:-cats}", interpolate("${:-cats}", &vars).unwrap());
        assert_eq!("", interpolate("${NOT:-}", &vars).unwrap());
        assert_eq!("cats", interpolate("${NOT-cats}", &vars).unwrap());
        assert_eq!("", interpolate("${EMPTY-cats}", &vars).unwrap());
        assert_eq!("dogs", interpolate("${FOO:?error cats}", &vars).unwrap());
        assert_eq!("dogs", interpolate("${FOO?error cats}", &vars).unwrap());
        assert_eq!("", interpolate("${EMPTY?error cats}", &vars).unwrap());
        assert!(interpolate("${NOT:?error cats}", &vars).is_err());
        assert!(interpolate("${NOT?error cats}", &vars).is_err());
        assert!(interpolate("${EMPTY:?error cats}", &vars).is_err());
    }

    #[test]
    fn multiline_values_cannot_change_the_parsed_structure() {
        let vars: HashMap<String, String> = vec![
            ("SAFE_VAR".into(), "single line value".into()),
            ("MULTILINE_VAR".into(), "line1\nline2\nline3".into()),
            ("WITH_NEWLINE".into(), "before\nafter".into()),
            ("WITH_CR".into(), "before\rafter".into()),
            ("WITH_CRLF".into(), "before\r\nafter".into()),
        ]
        .into_iter()
        .collect();

        for (name, value) in &vars {
            let input = serde_json::json!({"key": format!("${{{name}:-default}}")});
            let result =
                interpolate_config_map_with_env_vars(input.as_object().unwrap(), &vars).unwrap();
            assert_eq!(
                serde_json::Value::Object(result),
                serde_json::json!({"key": value})
            );
        }
    }
}
