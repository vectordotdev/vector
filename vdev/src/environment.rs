use regex::Regex;
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::OnceLock;

pub type Environment = BTreeMap<String, Option<String>>;

pub(crate) fn rename_environment_keys(environment: &Environment) -> Environment {
    environment
        .iter()
        .map(|(var, value)| {
            (
                format!("CONFIG_{}", var.replace('-', "_").to_uppercase()),
                value.clone(),
            )
        })
        .collect()
}

pub(crate) fn extract_present(environment: &Environment) -> BTreeMap<String, String> {
    environment
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|s| (k.clone(), s.clone())))
        .collect()
}

pub(crate) fn append_environment_variables(command: &mut Command, environment: &Environment) {
    for (key, value) in environment {
        command.arg("--env");
        match value {
            Some(value) => command.arg(format!("{key}={value}")),
            None => command.arg(key),
        };
    }
}

/// If the variable is not found or is `None`, it is left unchanged.
pub fn resolve_placeholders(input: &str, environment: &Environment) -> String {
    // static regexes
    static BRACED: OnceLock<Regex> = OnceLock::new();
    static BARE: OnceLock<Regex> = OnceLock::new();

    let braced = BRACED.get_or_init(|| Regex::new(r"\$\{([A-Za-z0-9_]+)\}").unwrap());
    let bare = BARE.get_or_init(|| Regex::new(r"\$([A-Za-z0-9_]+)").unwrap());

    // First replace ${VAR}
    let step1 = braced.replace_all(input, |caps: &regex::Captures| {
        let name = &caps[1];
        lookup(name, environment).unwrap_or_else(|| caps[0].to_string())
    });

    // Then replace $VAR
    bare.replace_all(&step1, |caps: &regex::Captures| {
        let name = &caps[1];
        lookup(name, environment).unwrap_or_else(|| caps[0].to_string())
    })
    .into_owned()
}

fn lookup(name: &str, env: &Environment) -> Option<String> {
    env.get(name)
        .and_then(std::clone::Clone::clone)
        .or_else(|| std::env::var(name).ok())
}
