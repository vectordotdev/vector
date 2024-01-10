use std::collections::{BTreeMap, BTreeSet};

use lookup::OwnedTargetPath;
use vrl::value::Kind;

use crate::config::LogNamespace;

use super::Definition;

/// The input schema for a given component.
///
/// This schema defines the (semantic) fields a component expects to receive from its input
/// components.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// Semantic meanings configured for this requirement.
    meaning: BTreeMap<String, SemanticMeaning>,
}

/// The semantic meaning of an event.
#[derive(Debug, Clone, PartialEq)]
struct SemanticMeaning {
    /// The type required by this semantic meaning.
    kind: Kind,

    /// Whether the meaning is optional.
    ///
    /// If a meaning is optional, the sink must not error when the meaning is not defined in the
    /// provided `Definition`, but it *must* error if it is defined, but its type does not meet the
    /// requirement.
    optional: bool,
}

impl Requirement {
    /// Create a new empty schema.
    ///
    /// An empty schema is the most "open" schema, in that there are no restrictions.
    pub fn empty() -> Self {
        Self {
            meaning: BTreeMap::default(),
        }
    }

    /// Check if the requirement is "empty", meaning:
    ///
    /// 1. There are no required fields defined.
    /// 2. The unknown fields are set to "any".
    /// 3. There are no required meanings defined.
    pub fn is_empty(&self) -> bool {
        self.meaning.is_empty()
    }

    /// Add a restriction to the schema.
    #[must_use]
    pub fn required_meaning(mut self, meaning: impl Into<String>, kind: Kind) -> Self {
        self.insert_meaning(meaning, kind, false);
        self
    }

    /// Add an optional restriction to the schema.
    ///
    /// This differs from `required_meaning` in that it is valid for the event to not have the
    /// specified meaning defined, but invalid for that meaning to be defined, but its [`Kind`] not
    /// matching the configured expectation.
    #[must_use]
    pub fn optional_meaning(mut self, meaning: impl Into<String>, kind: Kind) -> Self {
        self.insert_meaning(meaning, kind, true);
        self
    }

    fn insert_meaning(&mut self, identifier: impl Into<String>, kind: Kind, optional: bool) {
        let meaning = SemanticMeaning { kind, optional };
        self.meaning.insert(identifier.into(), meaning);
    }

    /// Validate the provided [`Definition`] against the current requirement.
    /// If `validate_schema_type` is true, validation ensure the types match,
    /// otherwise it only ensures the required fields exist.
    ///
    /// # Errors
    ///
    /// Returns a list of errors if validation fails.
    pub fn validate(
        &self,
        definition: &Definition,
        validate_schema_type: bool,
    ) -> Result<(), ValidationErrors> {
        let mut errors = vec![];

        // We only validate definitions if there is at least one connected component
        // that uses the Vector namespace.
        if !definition.log_namespaces().contains(&LogNamespace::Vector) {
            return Ok(());
        }

        for (identifier, req_meaning) in &self.meaning {
            // Check if we're dealing with an invalid meaning, meaning the definition has a single
            // meaning identifier pointing to multiple paths.
            if let Some(paths) = definition.invalid_meaning(identifier).cloned() {
                errors.push(ValidationError::MeaningDuplicate {
                    identifier: identifier.clone(),
                    paths,
                });
                continue;
            }

            let maybe_meaning_path = definition.meanings().find_map(|(def_id, path)| {
                if def_id == identifier {
                    Some(path)
                } else {
                    None
                }
            });

            match maybe_meaning_path {
                Some(target_path) if validate_schema_type => {
                    // Get the kind at the path for the given semantic meaning.
                    let definition_kind = definition.kind_at(target_path);

                    if req_meaning.kind.is_superset(&definition_kind).is_err() {
                        // The semantic meaning kind does not match the expected
                        // kind, so we can't use it in the sink.
                        errors.push(ValidationError::MeaningKind {
                            identifier: identifier.clone(),
                            want: req_meaning.kind.clone(),
                            got: definition_kind,
                        });
                    }
                }
                None if !req_meaning.optional => {
                    errors.push(ValidationError::MeaningMissing {
                        identifier: identifier.clone(),
                    });
                }
                _ => {}
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors(errors))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    pub fn is_meaning_missing(&self) -> bool {
        self.0.iter().any(ValidationError::is_meaning_missing)
    }

    pub fn is_meaning_kind(&self) -> bool {
        self.0.iter().any(ValidationError::is_meaning_kind)
    }

    pub fn errors(&self) -> &[ValidationError] {
        &self.0
    }
}

impl std::error::Error for ValidationErrors {
    fn source(&self) -> Option<&(dyn snafu::Error + 'static)> {
        Some(&self.0[0])
    }
}

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for error in &self.0 {
            error.fmt(f)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum ValidationError {
    /// A required semantic meaning is missing.
    MeaningMissing { identifier: String },

    /// A semantic meaning has an invalid `[Kind]`.
    MeaningKind {
        identifier: String,
        want: Kind,
        got: Kind,
    },

    /// A semantic meaning is pointing to multiple paths.
    MeaningDuplicate {
        identifier: String,
        paths: BTreeSet<OwnedTargetPath>,
    },
}

impl ValidationError {
    pub fn is_meaning_missing(&self) -> bool {
        matches!(self, Self::MeaningMissing { .. })
    }

    pub fn is_meaning_kind(&self) -> bool {
        matches!(self, Self::MeaningKind { .. })
    }

    pub fn is_meaning_duplicate(&self) -> bool {
        matches!(self, Self::MeaningDuplicate { .. })
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MeaningMissing { identifier } => {
                write!(f, "missing semantic meaning: {identifier}")
            }
            Self::MeaningKind {
                identifier,
                want,
                got,
            } => write!(
                f,
                "invalid semantic meaning: {identifier} (expected {want}, got {got})"
            ),
            Self::MeaningDuplicate { identifier, paths } => write!(
                f,
                "semantic meaning {} pointing to multiple fields: {}",
                identifier,
                paths
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod tests {
    use lookup::lookup_v2::parse_target_path;
    use lookup::owned_value_path;
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_doesnt_validate_types() {
        let requirement = Requirement::empty().required_meaning("foo", Kind::boolean());
        let definition = Definition::default_for_namespace(&[LogNamespace::Vector].into())
            .with_event_field(&owned_value_path!("foo"), Kind::integer(), Some("foo"));

        assert_eq!(Ok(()), requirement.validate(&definition, false));
    }

    #[test]
    fn test_doesnt_validate_legacy_namespace() {
        let requirement = Requirement::empty().required_meaning("foo", Kind::boolean());

        // We get an error if we have a connected component with the Vector namespace.
        let definition =
            Definition::default_for_namespace(&[LogNamespace::Vector, LogNamespace::Legacy].into())
                .with_event_field(&owned_value_path!("foo"), Kind::integer(), Some("foo"));

        assert_ne!(Ok(()), requirement.validate(&definition, true));

        // We don't get an error if we have a connected component with just the Legacy namespace.
        let definition = Definition::default_for_namespace(&[LogNamespace::Legacy].into())
            .with_event_field(&owned_value_path!("foo"), Kind::integer(), Some("foo"));

        assert_eq!(Ok(()), requirement.validate(&definition, true));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_validate() {
        struct TestCase {
            requirement: Requirement,
            definition: Definition,
            errors: Vec<ValidationError>,
        }

        for (
            title,
            TestCase {
                requirement,
                definition,
                errors,
            },
        ) in HashMap::from([
            (
                "empty",
                TestCase {
                    requirement: Requirement::empty(),
                    definition: Definition::default_for_namespace(&[LogNamespace::Vector].into()),
                    errors: vec![],
                },
            ),
            (
                "missing required meaning",
                TestCase {
                    requirement: Requirement::empty().required_meaning("foo", Kind::any()),
                    definition: Definition::default_for_namespace(&[LogNamespace::Vector].into()),
                    errors: vec![ValidationError::MeaningMissing {
                        identifier: "foo".into(),
                    }],
                },
            ),
            (
                "missing required meanings",
                TestCase {
                    requirement: Requirement::empty()
                        .required_meaning("foo", Kind::any())
                        .required_meaning("bar", Kind::any()),
                    definition: Definition::default_for_namespace(&[LogNamespace::Vector].into()),
                    errors: vec![
                        ValidationError::MeaningMissing {
                            identifier: "bar".into(),
                        },
                        ValidationError::MeaningMissing {
                            identifier: "foo".into(),
                        },
                    ],
                },
            ),
            (
                "missing optional meaning",
                TestCase {
                    requirement: Requirement::empty().optional_meaning("foo", Kind::any()),
                    definition: Definition::default_for_namespace(&[LogNamespace::Vector].into()),
                    errors: vec![],
                },
            ),
            (
                "missing mixed meanings",
                TestCase {
                    requirement: Requirement::empty()
                        .optional_meaning("foo", Kind::any())
                        .required_meaning("bar", Kind::any()),
                    definition: Definition::default_for_namespace(&[LogNamespace::Vector].into()),
                    errors: vec![ValidationError::MeaningMissing {
                        identifier: "bar".into(),
                    }],
                },
            ),
            (
                "invalid required meaning kind",
                TestCase {
                    requirement: Requirement::empty().required_meaning("foo", Kind::boolean()),
                    definition: Definition::default_for_namespace(&[LogNamespace::Vector].into())
                        .with_event_field(&owned_value_path!("foo"), Kind::integer(), Some("foo")),
                    errors: vec![ValidationError::MeaningKind {
                        identifier: "foo".into(),
                        want: Kind::boolean(),
                        got: Kind::integer(),
                    }],
                },
            ),
            (
                "invalid optional meaning kind",
                TestCase {
                    requirement: Requirement::empty().optional_meaning("foo", Kind::boolean()),
                    definition: Definition::default_for_namespace(&[LogNamespace::Vector].into())
                        .with_event_field(&owned_value_path!("foo"), Kind::integer(), Some("foo")),
                    errors: vec![ValidationError::MeaningKind {
                        identifier: "foo".into(),
                        want: Kind::boolean(),
                        got: Kind::integer(),
                    }],
                },
            ),
            (
                "duplicate meaning pointers",
                TestCase {
                    requirement: Requirement::empty().optional_meaning("foo", Kind::boolean()),
                    definition: Definition::default_for_namespace(&[LogNamespace::Vector].into())
                        .with_event_field(&owned_value_path!("foo"), Kind::integer(), Some("foo"))
                        .merge(
                            Definition::default_for_namespace(&[LogNamespace::Vector].into())
                                .with_event_field(
                                    &owned_value_path!("bar"),
                                    Kind::boolean(),
                                    Some("foo"),
                                ),
                        ),
                    errors: vec![ValidationError::MeaningDuplicate {
                        identifier: "foo".into(),
                        paths: BTreeSet::from([
                            parse_target_path("foo").unwrap(),
                            parse_target_path("bar").unwrap(),
                        ]),
                    }],
                },
            ),
        ]) {
            let got = requirement.validate(&definition, true);
            let want = if errors.is_empty() {
                Ok(())
            } else {
                Err(ValidationErrors(errors))
            };

            assert_eq!(got, want, "{title}");
        }
    }
}
