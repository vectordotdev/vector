use once_cell::sync::{Lazy, OnceCell};
use vector_config::configurable_component;

static TELEMETRY: OnceCell<Telemetry> = OnceCell::new();
static TELEMETRY_DEFAULT: Lazy<Telemetry> = Lazy::new(Telemetry::default);

/// Loads the telemetry options from configurations and sets the global options.
/// Once this is done, configurations can be correctly loaded using configured
/// log schema defaults.
///
/// # Errors
///
/// This function will fail if the `builder` fails.
///
/// # Panics
///
/// If deny is set, will panic if telemetry has already been set.
pub fn init_telemetry(telemetry: Telemetry, deny_if_set: bool) {
    assert!(
        !(TELEMETRY.set(telemetry).is_err() && deny_if_set),
        "Couldn't set telemetry"
    );
}

/// Returns the telemetry configuration options.
pub fn telemetry() -> &'static Telemetry {
    TELEMETRY.get().unwrap_or(&TELEMETRY_DEFAULT)
}

/// Sets options for the telemetry that Vector emits.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[serde(default)]
pub struct Telemetry {
    #[configurable(derived)]
    tags: Tags,
}

impl Telemetry {
    /// Merge two `Telemetry` instances together.
    pub fn merge(&mut self, other: &Telemetry) {
        self.tags.service = self.tags.service || other.tags.service;
        self.tags.source = self.tags.source || other.tags.source;
    }

    /// Returns true if any of the tag options are true.
    pub fn has_tags(&self) -> bool {
        self.tags.service || self.tags.source
    }

    pub fn tags(&self) -> &Tags {
        &self.tags
    }
}

/// Configures whether to emit certain tags
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[serde(default)]
pub struct Tags {
    /// Emit the service tag.
    service: bool,

    /// Emit the source tag.
    source: bool,
}

impl Tags {
    /// Returns true if the `service` tag should be emitted
    /// in the `component_received_*` and `component_sent_*`
    /// telemetry.
    pub fn service(&self) -> bool {
        self.service
    }

    /// Returns true if the `source` tag should be emitted
    /// in the `component_received_*` and `component_sent_*`
    /// telemetry.
    pub fn source(&self) -> bool {
        self.source
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn partial_telemetry() {
        let toml = r#"
            source = true
        "#;
        toml::from_str::<Telemetry>(toml).unwrap();
    }
}
