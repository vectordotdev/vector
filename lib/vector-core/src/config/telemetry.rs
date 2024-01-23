use std::sync::OnceLock;

use once_cell::sync::Lazy;
use vector_common::request_metadata::GroupedCountByteSize;
use vector_config::configurable_component;

static TELEMETRY: OnceLock<Telemetry> = OnceLock::new();
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
    pub tags: Tags,
}

impl Telemetry {
    /// Merge two `Telemetry` instances together.
    pub fn merge(&mut self, other: &Telemetry) {
        self.tags.emit_service = self.tags.emit_service || other.tags.emit_service;
        self.tags.emit_source = self.tags.emit_source || other.tags.emit_source;
    }

    /// Returns true if any of the tag options are true.
    pub fn has_tags(&self) -> bool {
        self.tags.emit_service || self.tags.emit_source
    }

    pub fn tags(&self) -> &Tags {
        &self.tags
    }

    /// The variant of `GroupedCountByteSize`
    pub fn create_request_count_byte_size(&self) -> GroupedCountByteSize {
        if self.has_tags() {
            GroupedCountByteSize::new_tagged()
        } else {
            GroupedCountByteSize::new_untagged()
        }
    }
}

/// Configures whether to emit certain tags
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[serde(default)]
pub struct Tags {
    /// True if the `service` tag should be emitted
    /// in the `component_received_*` and `component_sent_*`
    /// telemetry.
    pub emit_service: bool,

    /// True if the `source` tag should be emitted
    /// in the `component_received_*` and `component_sent_*`
    /// telemetry.
    pub emit_source: bool,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn partial_telemetry() {
        let toml = r"
            emit_source = true
        ";
        toml::from_str::<Telemetry>(toml).unwrap();
    }
}
