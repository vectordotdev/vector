use vector_lib::configurable::configurable_component;

/// Content encoding.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    /// Plaintext.
    #[derivative(Default)]
    Text,

    /// Newline-delimited JSON.
    Ndjson,

    /// JSON.
    Json,

    /// Binary.
    Binary,
}
