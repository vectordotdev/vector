use vector_lib::{config::LogNamespace, configurable::configurable_component};

pub(crate) use crate::schema::Definition;

/// Schema options.
///
/// **Note:** The `enabled` and `validation` options are experimental and should only be enabled if you
/// understand the limitations. While the infrastructure exists for schema tracking and validation, the
/// full vision of automatic semantic field mapping and comprehensive schema enforcement was never fully
/// realized.
///
/// If you encounter issues with these features, please [report them here](https://github.com/vectordotdev/vector/issues/new?template=bug.yml).
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// When enabled, Vector tracks the schema (field types and structure) of events as they flow
    /// from sources through transforms to sinks. This allows Vector to understand what data each
    /// component receives and produces.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// When enabled, Vector validates that events flowing into each sink match the schema
    /// requirements of that sink. If a sink requires certain fields or types that are missing
    /// from the incoming events, Vector will report an error during configuration validation.
    ///
    /// This helps catch pipeline configuration errors early, before runtime.
    #[serde(default = "default_validation")]
    pub validation: bool,

    /// Controls how metadata is stored in log events.
    ///
    /// When set to `false` (legacy mode), metadata fields like `host`, `timestamp`, and `source_type`
    /// are stored as top-level fields alongside your log data.
    ///
    /// When set to `true` (Vector namespace mode), metadata is stored in a separate metadata namespace,
    /// keeping it distinct from your actual log data.
    ///
    /// See the [Log Namespacing guide](/guides/level-up/log_namespace/) for detailed information
    /// about when to use Vector namespace mode and how to migrate from legacy mode.
    pub log_namespace: Option<bool>,
}

impl Options {
    /// Gets the value of the globally configured log namespace, or the default if it wasn't set.
    pub fn log_namespace(self) -> LogNamespace {
        self.log_namespace
            .map_or(LogNamespace::Legacy, |use_vector_namespace| {
                use_vector_namespace.into()
            })
    }

    /// Merges two schema options together.
    pub fn append(&mut self, with: Self, errors: &mut Vec<String>) {
        if self.log_namespace.is_some()
            && with.log_namespace.is_some()
            && self.log_namespace != with.log_namespace
        {
            errors.push(
                format!("conflicting values for 'log_namespace' found. Both {:?} and {:?} used in the same component",
                        self.log_namespace(), with.log_namespace())
            );
        }
        if let Some(log_namespace) = with.log_namespace {
            self.log_namespace = Some(log_namespace);
        }

        // If either config enables these flags, it is enabled.
        self.enabled |= with.enabled;
        self.validation |= with.validation;
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            validation: default_validation(),
            log_namespace: None,
        }
    }
}

const fn default_enabled() -> bool {
    false
}

const fn default_validation() -> bool {
    false
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_append() {
        for (test, mut a, b, expected) in [
            (
                "enable log namespacing",
                Options {
                    enabled: false,
                    validation: false,
                    log_namespace: None,
                },
                Options {
                    enabled: false,
                    validation: false,
                    log_namespace: Some(true),
                },
                Some(Options {
                    enabled: false,
                    validation: false,
                    log_namespace: Some(true),
                }),
            ),
            (
                "log namespace conflict",
                Options {
                    enabled: false,
                    validation: false,
                    log_namespace: Some(false),
                },
                Options {
                    enabled: false,
                    validation: false,
                    log_namespace: Some(true),
                },
                None,
            ),
            (
                "enable schemas",
                Options {
                    enabled: false,
                    validation: false,
                    log_namespace: None,
                },
                Options {
                    enabled: true,
                    validation: false,
                    log_namespace: None,
                },
                Some(Options {
                    enabled: true,
                    validation: false,
                    log_namespace: None,
                }),
            ),
            (
                "enable sink requirements",
                Options {
                    enabled: false,
                    validation: false,
                    log_namespace: None,
                },
                Options {
                    enabled: false,
                    validation: true,
                    log_namespace: None,
                },
                Some(Options {
                    enabled: false,
                    validation: true,
                    log_namespace: None,
                }),
            ),
        ] {
            let mut errors = vec![];
            a.append(b, &mut errors);
            if errors.is_empty() {
                assert_eq!(Some(a), expected, "result mismatch: {test}");
            } else {
                assert_eq!(
                    errors.is_empty(),
                    expected.is_some(),
                    "error mismatch: {test}"
                );
            }
        }
    }
}
