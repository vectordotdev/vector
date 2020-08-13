use regex::{Captures, Regex};
use std::collections::HashMap;

pub fn interpolate(input: &str, vars: &HashMap<String, String>) -> String {
    let re = Regex::new(r"\$\$|\$(\w+)|\$\{(\w+)(?::-([^}]+)?)?\}").unwrap();
    re.replace_all(input, |caps: &Captures<'_>| {
        caps.get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str())
            .map(|name| {
                vars.get(name).map(|val| val.as_str()).unwrap_or_else(|| {
                    caps.get(3).map(|m| m.as_str()).unwrap_or_else(|| {
                        warn!("unknown env var in config: {:?}", name);
                        ""
                    })
                })
            })
            .unwrap_or("$")
            .to_string()
    })
    .into_owned()
}

#[cfg(test)]
mod test {
    use super::interpolate;
    #[test]
    fn interpolation() {
        let vars = vec![
            ("FOO".into(), "dogs".into()),
            ("FOOBAR".into(), "cats".into()),
        ]
        .into_iter()
        .collect();

        assert_eq!("dogs", interpolate("$FOO", &vars));
        assert_eq!("dogs", interpolate("${FOO}", &vars));
        assert_eq!("cats", interpolate("${FOOBAR}", &vars));
        assert_eq!("xcatsy", interpolate("x${FOOBAR}y", &vars));
        assert_eq!("x", interpolate("x$FOOBARy", &vars));
        assert_eq!("$ x", interpolate("$ x", &vars));
        assert_eq!("$FOO", interpolate("$$FOO", &vars));
        assert_eq!("", interpolate("$NOT_FOO", &vars));
        assert_eq!("-FOO", interpolate("$NOT-FOO", &vars));
        assert_eq!("${FOO x", interpolate("${FOO x", &vars));
        assert_eq!("${}", interpolate("${}", &vars));
        assert_eq!("dogs", interpolate("${FOO:-cats}", &vars));
        assert_eq!("dogcats", interpolate("${NOT:-dogcats}", &vars));
        assert_eq!("dogs and cats", interpolate("${NOT:-dogs and cats}", &vars));
        assert_eq!("${:-cats}", interpolate("${:-cats}", &vars));
        assert_eq!("", interpolate("${NOT:-}", &vars));
    }
}
