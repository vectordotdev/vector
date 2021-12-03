use regex::{Captures, Regex};
use std::collections::HashMap;

/// (result, warnings)
pub fn interpolate(input: &str, vars: &HashMap<String, String>) -> (String, Vec<String>) {
    let mut warnings = Vec::new();
    // Environment variable names can have any characters from the Portable Character Set other
    // than NUL.  However, for Vector's intepolation, we are closer to what a shell supports which
    // is solely of uppercase letters, digits, and the '_' (that is, the `[:word:]` regex class).
    // In addition to these characters, we allow `.` as this commonly appears in environment
    // variable names when they come from a Java properties file.
    //
    // https://pubs.opengroup.org/onlinepubs/000095399/basedefs/xbd_chap08.html
    let re = Regex::new(
        r"(?x)
        \$\$|
        \$([[:word:].]+)|
        \$\{([[:word:].]+)(?::-([^}]+)?)?\}",
    )
    .unwrap();
    let interpolated = re
        .replace_all(input, |caps: &Captures<'_>| {
            caps.get(1)
                .or_else(|| caps.get(2))
                .map(|m| m.as_str())
                .map(|name| {
                    vars.get(name).map(|val| val.as_str()).unwrap_or_else(|| {
                        caps.get(3).map(|m| m.as_str()).unwrap_or_else(|| {
                            warnings.push(format!("Unknown env var in config. name = {:?}", name));
                            ""
                        })
                    })
                })
                .unwrap_or("$")
                .to_string()
        })
        .into_owned();
    (interpolated, warnings)
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
        ]
        .into_iter()
        .collect();

        assert_eq!("dogs", interpolate("$FOO", &vars).0);
        assert_eq!("dogs", interpolate("${FOO}", &vars).0);
        assert_eq!("cats", interpolate("${FOOBAR}", &vars).0);
        assert_eq!("xcatsy", interpolate("x${FOOBAR}y", &vars).0);
        assert_eq!("x", interpolate("x$FOOBARy", &vars).0);
        assert_eq!("$ x", interpolate("$ x", &vars).0);
        assert_eq!("$FOO", interpolate("$$FOO", &vars).0);
        assert_eq!("dogs=bar", interpolate("$FOO=bar", &vars).0);
        assert_eq!("", interpolate("$NOT_FOO", &vars).0);
        assert_eq!("-FOO", interpolate("$NOT-FOO", &vars).0);
        assert_eq!("turtles", interpolate("$FOO.BAR", &vars).0);
        assert_eq!("${FOO x", interpolate("${FOO x", &vars).0);
        assert_eq!("${}", interpolate("${}", &vars).0);
        assert_eq!("dogs", interpolate("${FOO:-cats}", &vars).0);
        assert_eq!("dogcats", interpolate("${NOT:-dogcats}", &vars).0);
        assert_eq!(
            "dogs and cats",
            interpolate("${NOT:-dogs and cats}", &vars).0
        );
        assert_eq!("${:-cats}", interpolate("${:-cats}", &vars).0);
        assert_eq!("", interpolate("${NOT:-}", &vars).0);
    }
}
