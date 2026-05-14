use vector_lib::configurable::configurable_component;

/// Content encoding.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// Plaintext.
    #[default]
    Text,

    /// Newline-delimited JSON.
    Ndjson,

    /// JSON.
    Json,

    /// Binary.
    Binary,
}
