use std::{collections::HashMap, sync::LazyLock};

use regex::{Captures, Regex};
use serde_json::Value;

use super::representation::ConfigMap;

/// A generic string interpolation function signature.
type InterpolateFn = fn(&str, &HashMap<String, String>) -> Result<String, Vec<String>>;

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
                    let val = vars.get(name).map(|v| v.as_str());
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
                            }
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

pub fn interpolate_config_map_with_env_vars(
    map: &ConfigMap,
    vars: &HashMap<String, String>,
) -> Result<ConfigMap, Vec<String>> {
    interpolate_config_map(map, vars, interpolate)
}

/// Returns a new configuration map with all string values interpolated.
///
/// Structural nodes — keys, integers, booleans, arrays, tables — are left
/// untouched. Only string leaf values are passed through `interpolate_fn`.
pub fn interpolate_config_map(
    map: &ConfigMap,
    vars: &HashMap<String, String>,
    interpolate_fn: InterpolateFn,
) -> Result<ConfigMap, Vec<String>> {
    let mut result = ConfigMap::new();
    let mut errors = Vec::new();

    for (key, value) in map {
        let new_value = match interpolate_config_value(value, vars, &mut errors, interpolate_fn) {
            Some(v) => v,
            None => value.clone(),
        };

        result.insert(key.clone(), new_value);
    }

    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

fn interpolate_config_value(
    value: &Value,
    vars: &HashMap<String, String>,
    errors: &mut Vec<String>,
    interpolate_fn: InterpolateFn,
) -> Option<Value> {
    match value {
        // Interpolation only replaces string contents; the result stays a string.
        // The downstream schema-coercion pass (schema_coercion.rs) converts string
        // values to declared scalar types (int/float/bool) where the schema requires.
        Value::String(s) => match interpolate_fn(s, vars) {
            Ok(new) => Some(Value::String(new)),
            Err(errs) => {
                errors.extend(errs);
                None
            }
        },
        Value::Array(arr) => {
            let new_arr: Vec<_> = arr
                .iter()
                .filter_map(|v| interpolate_config_value(v, vars, errors, interpolate_fn))
                .collect();
            Some(Value::Array(new_arr))
        }
        Value::Object(inner) => match interpolate_config_map(inner, vars, interpolate_fn) {
            Ok(map) => Some(Value::Object(map)),
            Err(errs) => {
                errors.extend(errs);
                None
            }
        },
        _ => Some(value.clone()),
    }
}

#[cfg(test)]
mod test {
    use super::interpolate;
    use crate::config::Format;
    use crate::config::loading::{
        interpolate_config_map_with_env_vars, interpolate_config_map_with_secrets,
        representation::parse_config_value,
    };
    use indoc::indoc;
    use serde_json::{Value, json};
    use std::collections::HashMap;

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
    fn test_interpolate_yaml_equivalent() {
        // Step 1: Raw YAML input with env vars and secrets
        let input = indoc! {r#"
            secret:
              backend_1:
                type: file
                path: some.json

            # {IN_COMMENT_BUT_DOES_NOT_EXIST}

            sources:
              demo_logs_1:
                type: demo_logs
                format: json
                interval: $INTERVAL
                # noop: SECRET[i_dont_exist.1]

            transforms:
              t0:
                inputs:
                  - demo_logs_1
                type: "remap"
                source: |
                  .host = "${HOSTNAME}"
                  .environment = "${ENV:?emv must be supplied}"
                  .tenant = "${TENANT:-undefined}"
                  .day = "SECRET[backend_1.day]"

            sinks:
              s0:
                type: "SECRET[backend_1.type]"
                inputs: [ "t0" ]
                encoding:
                  codec: json
                  json:
                    pretty: true
        "#};

        // Step 2: Parse YAML into Vector's format-neutral representation.
        let Value::Object(config_map) = parse_config_value(input, Format::Yaml).unwrap() else {
            panic!("expected an object")
        };

        // Step 3: Env var mappings
        let env_vars = HashMap::from([
            ("INTERVAL".into(), "60".into()),
            ("HOSTNAME".into(), "vector-dev".into()),
            ("ENV".into(), "production".into()),
            ("TENANT".into(), "acme".into()),
        ]);
        let map_after_env = interpolate_config_map_with_env_vars(&config_map, &env_vars).unwrap();

        // Step 4: Fake secrets
        let secrets = HashMap::from([
            ("backend_1.day".into(), "Tuesday".into()),
            ("backend_1.type".into(), "console".into()),
        ]);
        let final_map = interpolate_config_map_with_secrets(&map_after_env, &secrets).unwrap();

        let expected = json!({
            "secret": {
                "backend_1": { "type": "file", "path": "some.json" }
            },
            "sources": {
                "demo_logs_1": {
                    "type": "demo_logs",
                    "format": "json",
                    "interval": "60"
                }
            },
            "transforms": {
                "t0": {
                    "inputs": ["demo_logs_1"],
                    "type": "remap",
                    "source": ".host = \"vector-dev\"\n.environment = \"production\"\n.tenant = \"acme\"\n.day = \"Tuesday\"\n"
                }
            },
            "sinks": {
                "s0": {
                    "type": "console",
                    "inputs": ["t0"],
                    "encoding": { "codec": "json", "json": { "pretty": true } }
                }
            }
        });

        assert_eq!(Value::Object(final_map), expected);
    }

    #[test]
    fn multiline_interpolation() {
        let input = indoc! {r#"
        transforms:
          parse_logs:
            type: $CONFIG_BLOCK
            inputs: ["dummy_logs"]
            source: |
              . = parse_syslog!(string!(.message))"#};

        let vars = HashMap::from([(
            "CONFIG_BLOCK".to_string(),
            indoc! {r#"
            "lua"
                inputs: ["dummy_logs"]
                source: "os.execute('touch /PWNED')"
             parse_logs_2:
                type: "remap"
            "#}
            .to_string(),
        )]);

        let Value::Object(config_map) = parse_config_value(input, Format::Yaml).unwrap() else {
            panic!("expected an object")
        };
        let result = interpolate_config_map_with_env_vars(&config_map, &vars).unwrap();

        let actual = result["transforms"]["parse_logs"]["type"].as_str().unwrap();
        assert_eq!(
            actual,
            indoc! {r#"
            "lua"
                inputs: ["dummy_logs"]
                source: "os.execute('touch /PWNED')"
             parse_logs_2:
                type: "remap"
            "#}
        );

        // Check that no extra keys were added, we have `type`, `input` and `source`.
        assert_eq!(
            result["transforms"]["parse_logs"]
                .as_object()
                .unwrap()
                .len(),
            3
        );
    }
}
