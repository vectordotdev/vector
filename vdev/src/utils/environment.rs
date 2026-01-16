use std::{collections::BTreeMap, process::Command};

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(unix)] {
        use std::sync::OnceLock;
        use regex::{Captures, Regex};
    }
}

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

cfg_if! {
if #[cfg(unix)] {
    /// Resolve all environment variable placeholders. If the variable is not found or is `None`, it is left unchanged.
    pub fn resolve_placeholders(input: &str, environment: &Environment) -> String {
        static BRACED: OnceLock<Regex> = OnceLock::new();
        static BARE: OnceLock<Regex> = OnceLock::new();

        let braced =
            BRACED.get_or_init(|| Regex::new(r"\$\{([A-Za-z0-9_]+)\}").expect("cannot build regex"));
        let bare = BARE.get_or_init(|| Regex::new(r"\$([A-Za-z0-9_]+)").expect("cannot build regex"));

        // First replace ${VAR}
        let step1 = braced.replace_all(input, |captures: &Captures| {
            resolve_or_keep(&captures[0], &captures[1], environment)
        });

        // Then replace $VAR
        bare.replace_all(&step1, |captures: &Captures| {
            resolve_or_keep(&captures[0], &captures[1], environment)
        })
        .into_owned()
    }

    #[cfg(unix)]
    fn resolve_or_keep(full: &str, name: &str, env: &Environment) -> String {
        env.get(name)
            .and_then(Clone::clone)
            .or_else(|| std::env::var(name).ok())
            .unwrap_or_else(|| full.to_string())
    }
}
}
