use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use vector_common::config::ComponentKey;

pub type DiagnosticResult<T> = Result<(T, Vec<WarningDiagnostic>), Vec<ErrorDiagnostic>>;

#[derive(Serialize)]
#[serde(untagged)]
pub enum DiagnosticSource {
    /// Originates from a specific component.
    ///
    /// For example, if a component was configured with a value that was not valid but was only
    /// checked at runtime -- perhaps due to needing to validate that value in the context of
    /// _other_ values -- then a component-sourced error could be emitted during the building of the
    /// component.
    ///
    /// Since this uses `ComponentKey`, the source may apply to a component or a specific output of a
    /// component.
    Component(ComponentKey),

    /// Originates from outside of a component.
    ///
    /// This is a catch-all source for when a validation step is not inherently scoped to a specific
    /// component: for example, checking the validity of any "global" options.
    #[serde(other)]
    Global,
}

#[derive(Serialize)]
pub enum DiagnosticLevel {
    /// An error.
    ///
    /// Errors represent validation issues that prevent Vector from being able to build and run a configuration.
    Error,

    /// A warning.
    ///
    /// Warnings represent validation issues that don't necessarily prevent Vector from being able
    /// to build and run a configuration. In general, most warnings represent _likely_ errors if
    /// Vector attempted to build and run a configuration but are marked as warnings due to a lack
    /// of full context.
    ///
    /// For example, if a configuration refers to specific environment variables, a warning might be
    /// emitted if one of the environment variables is missing during validation: Vector cannot be
    /// sure that the environment variable will not be present when the configuration is actually
    /// built and run, only that it was missing during validation.
    Warning,
}

struct DiagnosticInner {
    /// The source of the diagnostic.
    source: DiagnosticSource,

    /// The human-readable description of the diagnostic.
    desc: String,

    /// The context of the diagnostic.
    context: HashMap<String, Value>,
    }

#[derive(Serialize)]
pub struct ValidationDiagnostic {
    /// The level of the diagnostic.
    level: DiagnosticLevel,

    #[serde(flatten)]
    inner: DiagnosticInner,
}

impl ValidationDiagnostic {
    /// Creates an error diagnostic with the given description.
    ///
    /// The diagnostic has a default source of "global".
    pub fn error(desc: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            inner: DiagnosticInner {
                source: DiagnosticSource::Global,
                desc: desc.into(),
                context: HashMap::new(),
            }
        }
    }

    /// Creates a warning diagnostic with the given description.
    ///
    /// The diagnostic has a default source of "global".
    pub fn warning(desc: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            inner: DiagnosticInner {
                source: DiagnosticSource::Global,
                desc: desc.into(),
                context: HashMap::new(),
            }
        }
    }

    /// Sets the diagnostic source to the given component.
    pub fn from_component(mut self, source: impl Into<ComponentKey>) -> Self {
        self.inner.source = DiagnosticSource::Component(source.into());
        self
    }

    /// Adds the given context data to the diagnostic.
    ///
    /// If the given context key already exists, it is overwritten.
    pub fn with_context(mut self, context_key: &str, context_data: impl Serialize) -> Self {
        let context_data = serde_json::to_value(context_data).expect("should not fail to serialize");
        self.inner.context.insert(context_key.to_string(), context_data);
        self
    }
}

pub struct ErrorDiagnostic(DiagnosticInner);

impl ErrorDiagnostic {
    /// Gets the source of the error diagnostic.
    pub fn source(&self) -> &DiagnosticSource {
        &self.0.source
    }

    /// Gets the description of the error diagnostic.
    pub fn description(&self) -> &str {
        self.0.desc.as_str()
    }

    /// Gets the context of the error diagnostic.
    pub fn context(&self) -> &HashMap<String, Value> {
        &self.0.context
    }
}

pub struct WarningDiagnostic(DiagnosticInner);

impl WarningDiagnostic {
    /// Gets the source of the warning diagnostic.
    pub fn source(&self) -> &DiagnosticSource {
        &self.0.source
    }

    /// Gets the description of the warning diagnostic.
    pub fn description(&self) -> &str {
        self.0.desc.as_str()
    }

    /// Gets the context of the warning diagnostic.
    pub fn context(&self) -> &HashMap<String, Value> {
        &self.0.context
    }
}

/// Accumulates validation diagnostics and provides helper methods for short-circuiting validation
/// if errors are encountered.
#[derive(Default, Serialize)]
pub struct DiagnosticAccumulator {
    diagnostics: Vec<ValidationDiagnostic>,
}

impl DiagnosticAccumulator {
    /// Adds a diagnostic to the accumulator.
    pub fn push(&mut self, result: ValidationDiagnostic) {
        self.diagnostics.push(result);
    }

    /// Stops accumulating diagnostics, and returns either the accumulated warnings or errors.
    ///
    /// If any errors were accumulated, then all errors are returned as `Err` and the warnings are
    /// discarded. Otherwise, `Ok` is returned with the accumulated warnings, if any.
    pub fn finish(self) -> Result<Vec<WarningDiagnostic>, Vec<ErrorDiagnostic>> {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        for diagnostic in self.diagnostics {
            match diagnostic.level {
                DiagnosticLevel::Error => errors.push(ErrorDiagnostic(diagnostic.inner)),
                DiagnosticLevel::Warning => warnings.push(WarningDiagnostic(diagnostic.inner)),
            }
        }

        if errors.is_empty() {
            Ok(warnings)
        } else {
            Err(errors)
        }
    }
}
