use serde::Serialize;
use vector_lib::configurable::configurable_component;

/// Data retained by test components for configuration-loading assertions.
#[configurable_component]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct TestComponentData {
    /// String values whose contents must survive parsing and substitution.
    pub strings: Vec<String>,

    /// An integer value used to exercise schema-driven coercion.
    pub integer: Option<usize>,
}

impl TestComponentData {
    pub const DEFAULT: Self = Self {
        strings: Vec::new(),
        integer: None,
    };

    /// Reads the extra data from a loaded test source or sink configuration.
    /// Returns the default data when it was omitted.
    ///
    /// # Panics
    ///
    /// Panics if the configuration cannot be serialized or contains invalid extra data.
    #[must_use]
    pub fn from_config(config: &impl Serialize) -> Self {
        let value =
            serde_json::to_value(config).expect("test component configuration must serialize");
        value.get("extra").map_or_else(Self::default, |extra| {
            serde_json::from_value(extra.clone())
                .expect("test component configuration must contain valid extra data")
        })
    }
}
