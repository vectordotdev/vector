use std::{collections::HashMap, sync::LazyLock};

use regex::{Captures, Regex};
use toml::{Table, Value};

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

pub fn interpolate_toml_table_with_env_vars(
    table: &Table,
    vars: &HashMap<String, String>,
) -> Result<Table, Vec<String>> {
    interpolate_toml_table(table, vars, interpolate)
}

/// Returns a new TOML `Table` with all string values interpolated.
pub fn interpolate_toml_table(
    table: &Table,
    vars: &HashMap<String, String>,
    interpolate_fn: InterpolateFn,
) -> Result<Table, Vec<String>> {
    let mut result = Table::new();
    let mut errors = Vec::new();

    for (key, value) in table {
        let new_key = match interpolate_fn(key, vars) {
            Ok(k) => k,
            Err(errs) => {
                errors.extend(errs);
                key.clone()
            }
        };

        let new_value = match interpolate_toml_value(value, vars, &mut errors, interpolate_fn) {
            Some(v) => v,
            None => value.clone(),
        };

        result.insert(new_key, new_value);
    }

    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

fn interpolate_toml_value(
    value: &Value,
    vars: &HashMap<String, String>,
    errors: &mut Vec<String>,
    interpolate_fn: InterpolateFn,
) -> Option<Value> {
    match value {
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
                .filter_map(|v| interpolate_toml_value(v, vars, errors, interpolate_fn))
                .collect();
            Some(Value::Array(new_arr))
        }
        Value::Table(inner) => match interpolate_toml_table(inner, vars, interpolate_fn) {
            Ok(t) => Some(Value::Table(t)),
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
    use crate::config::loading::{
        interpolate_toml_table_with_env_vars, interpolate_toml_table_with_secrets,
    };
    use indoc::indoc;
    use std::collections::HashMap;
    use toml::{Table, Value};

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
    fn test_interpolate_toml_table() {
        let raw = indoc! {r#"
        # ${IN_COMMENT_BUT_DOES_NOT_EXIST}
        [[tests]]
            name = "${NAME}"
            # This line should not cause a loading failure - "SECRET[backend_1.i_dont_exist]"
            [[tests.inputs]]
                insert_at = "${UNDEFINED:-foo}"
                type = "log"
                [tests.inputs.log_fields]
                  "$FIELD1" = 1
                  "$FIELD2" = 2
                  top_secret = "SECRET[backend_1.some_secret]"
        "#};

        let root_value: Value = toml::from_str(raw).expect("valid toml");
        let root_table = match root_value {
            Value::Table(t) => t,
            _ => panic!("expected root table"),
        };

        let vars = HashMap::from([
            ("NAME".into(), "Some_Transform".into()),
            ("FIELD1".into(), "f1".into()),
            ("FIELD2".into(), "f2".into()),
        ]);

        let result = interpolate_toml_table_with_env_vars(&root_table, &vars).unwrap();

        let secrets = HashMap::from([("backend_1.some_secret".into(), "foo".into())]);
        let result = interpolate_toml_table_with_secrets(&result, &secrets).unwrap();

        let expected: Value = toml::from_str(indoc! {r#"
            [[tests]]
                name = "Some_Transform"
                [[tests.inputs]]
                    insert_at = "foo"
                    type = "log"
                    [tests.inputs.log_fields]
                    f1 = 1
                    f2 = 2
                    top_secret = "foo"
        "#})
        .unwrap();
        assert_eq!(Value::Table(result), expected);
    }

    #[test]
    fn multiline_interpolation() {
        let raw = indoc! {r#"
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

        let yaml_value: Value = serde_yaml::from_str(raw).unwrap();
        let toml_table: Table = yaml_value.try_into().unwrap();
        let result = interpolate_toml_table_with_env_vars(&toml_table, &vars).unwrap();

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
            result["transforms"]["parse_logs"].as_table().unwrap().len(),
            3
        );
    }
}
