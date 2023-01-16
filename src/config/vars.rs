use std::collections::HashMap;

use regex::{Captures, Regex};

/// (result, warnings)
pub fn interpolate(
    input: &str,
    vars: &HashMap<String, String>,
) -> Result<(String, Vec<String>), Vec<String>> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Environment variable names can have any characters from the Portable Character Set other
    // than NUL.  However, for Vector's interpolation, we are closer to what a shell supports which
    // is solely of uppercase letters, digits, and the '_' (that is, the `[:word:]` regex class).
    // In addition to these characters, we allow `.` as this commonly appears in environment
    // variable names when they come from a Java properties file.
    //
    // https://pubs.opengroup.org/onlinepubs/000095399/basedefs/xbd_chap08.html
    let re = Regex::new(
        r"(?x)
        \$\$|
        \$([[:word:].]+)|
        \$\{([[:word:].]+)(?:(:?-|:?\?)([^}]*))?\}",
    )
    .unwrap();

    let interpolated = re
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
                                    "Non-empty env var required in config. name = {:?}, error = {:?}",
                                    name, def_or_err
                                ));
                                ""
                            },
                        }
                        "?" => val.unwrap_or_else(|| {
                            errors.push(format!(
                                "Missing env var required in config. name = {:?}, error = {:?}",
                                name, def_or_err
                            ));
                            ""
                        }),
                        _ => val.unwrap_or_else(|| {
                            warnings
                                .push(format!("Unknown env var in config. name = {:?}", name));
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

        assert_eq!("dogs", interpolate("$FOO", &vars).unwrap().0);
        assert_eq!("dogs", interpolate("${FOO}", &vars).unwrap().0);
        assert_eq!("cats", interpolate("${FOOBAR}", &vars).unwrap().0);
        assert_eq!("xcatsy", interpolate("x${FOOBAR}y", &vars).unwrap().0);
        assert_eq!("x", interpolate("x$FOOBARy", &vars).unwrap().0);
        assert_eq!("$ x", interpolate("$ x", &vars).unwrap().0);
        assert_eq!("$FOO", interpolate("$$FOO", &vars).unwrap().0);
        assert_eq!("dogs=bar", interpolate("$FOO=bar", &vars).unwrap().0);
        assert_eq!("", interpolate("$NOT_FOO", &vars).unwrap().0);
        assert_eq!("-FOO", interpolate("$NOT-FOO", &vars).unwrap().0);
        assert_eq!("turtles", interpolate("$FOO.BAR", &vars).unwrap().0);
        assert_eq!("${FOO x", interpolate("${FOO x", &vars).unwrap().0);
        assert_eq!("${}", interpolate("${}", &vars).unwrap().0);
        assert_eq!("dogs", interpolate("${FOO:-cats}", &vars).unwrap().0);
        assert_eq!("dogcats", interpolate("${NOT:-dogcats}", &vars).unwrap().0);
        assert_eq!(
            "dogs and cats",
            interpolate("${NOT:-dogs and cats}", &vars).unwrap().0
        );
        assert_eq!("${:-cats}", interpolate("${:-cats}", &vars).unwrap().0);
        assert_eq!("", interpolate("${NOT:-}", &vars).unwrap().0);
        assert_eq!("cats", interpolate("${NOT-cats}", &vars).unwrap().0);
        assert_eq!("", interpolate("${EMPTY-cats}", &vars).unwrap().0);
        assert_eq!("dogs", interpolate("${FOO:?error cats}", &vars).unwrap().0);
        assert_eq!("dogs", interpolate("${FOO?error cats}", &vars).unwrap().0);
        assert_eq!("", interpolate("${EMPTY?error cats}", &vars).unwrap().0);
        assert!(interpolate("${NOT:?error cats}", &vars).is_err());
        assert!(interpolate("${NOT?error cats}", &vars).is_err());
        assert!(interpolate("${EMPTY:?error cats}", &vars).is_err());
    }
}
