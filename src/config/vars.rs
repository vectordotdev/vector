use std::collections::HashMap;

use once_cell::sync::Lazy;
use regex::{Captures, Regex};

// Environment variable names can have any characters from the Portable Character Set other
// than NUL.  However, for Vector's interpolation, we are closer to what a shell supports which
// is solely of uppercase letters, digits, and the '_' (that is, the `[:word:]` regex class).
// In addition to these characters, we allow `.` as this commonly appears in environment
// variable names when they come from a Java properties file.
//
// https://pubs.opengroup.org/onlinepubs/000095399/basedefs/xbd_chap08.html
pub static ENVIRONMENT_VARIABLE_INTERPOLATION_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        \$\$|
        \$([[:word:].]+)|
        \$\{([[:word:].]+)(?:(:?-|:?\?)([^}]*))?\}",
    )
    .unwrap()
});

/// (result, warnings)
pub fn interpolate(
    input: &str,
    vars: &HashMap<String, String>,
    strict_vars: bool,
) -> Result<(String, Vec<String>), Vec<String>> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

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
                            },
                        }
                        "?" => val.unwrap_or_else(|| {
                            errors.push(format!(
                                "Missing environment variable required in config. name = {name:?}, error = {def_or_err:?}",
                            ));
                            ""
                        }),
                        _ => val.unwrap_or_else(|| if strict_vars {
                            errors.push(format!(
                                "Missing environment variable in config. name = {name:?}",
                            ));
                            ""
                        } else {
                            warnings
                                .push(format!(
                                    "Unknown environment variable in config. This is DEPRECATED and will become an error in future versions. name = {name:?}",
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
        Ok((interpolated, warnings))
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod test {
    use super::interpolate;
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

        assert_eq!("dogs", interpolate("$FOO", &vars, true).unwrap().0);
        assert_eq!("dogs", interpolate("${FOO}", &vars, true).unwrap().0);
        assert_eq!("cats", interpolate("${FOOBAR}", &vars, true).unwrap().0);
        assert_eq!("xcatsy", interpolate("x${FOOBAR}y", &vars, true).unwrap().0);
        assert_eq!("x", interpolate("x$FOOBARy", &vars, false).unwrap().0);
        assert!(interpolate("x$FOOBARy", &vars, true).is_err());
        assert_eq!("$ x", interpolate("$ x", &vars, false).unwrap().0);
        assert_eq!("$ x", interpolate("$ x", &vars, true).unwrap().0);
        assert_eq!("$FOO", interpolate("$$FOO", &vars, true).unwrap().0);
        assert_eq!("dogs=bar", interpolate("$FOO=bar", &vars, true).unwrap().0);
        assert_eq!("", interpolate("$NOT_FOO", &vars, false).unwrap().0);
        assert!(interpolate("$NOT_FOO", &vars, true).is_err());
        assert_eq!("-FOO", interpolate("$NOT-FOO", &vars, false).unwrap().0);
        assert!(interpolate("$NOT-FOO", &vars, true).is_err());
        assert_eq!("turtles", interpolate("$FOO.BAR", &vars, true).unwrap().0);
        assert_eq!("${FOO x", interpolate("${FOO x", &vars, true).unwrap().0);
        assert_eq!("${}", interpolate("${}", &vars, true).unwrap().0);
        assert_eq!("dogs", interpolate("${FOO:-cats}", &vars, true).unwrap().0);
        assert_eq!(
            "dogcats",
            interpolate("${NOT:-dogcats}", &vars, true).unwrap().0
        );
        assert_eq!(
            "dogs and cats",
            interpolate("${NOT:-dogs and cats}", &vars, true).unwrap().0
        );
        assert_eq!(
            "${:-cats}",
            interpolate("${:-cats}", &vars, true).unwrap().0
        );
        assert_eq!("", interpolate("${NOT:-}", &vars, true).unwrap().0);
        assert_eq!("cats", interpolate("${NOT-cats}", &vars, true).unwrap().0);
        assert_eq!("", interpolate("${EMPTY-cats}", &vars, true).unwrap().0);
        assert_eq!(
            "dogs",
            interpolate("${FOO:?error cats}", &vars, true).unwrap().0
        );
        assert_eq!(
            "dogs",
            interpolate("${FOO?error cats}", &vars, true).unwrap().0
        );
        assert_eq!(
            "",
            interpolate("${EMPTY?error cats}", &vars, true).unwrap().0
        );
        assert!(interpolate("${NOT:?error cats}", &vars, true).is_err());
        assert!(interpolate("${NOT?error cats}", &vars, true).is_err());
        assert!(interpolate("${EMPTY:?error cats}", &vars, true).is_err());
    }
}
