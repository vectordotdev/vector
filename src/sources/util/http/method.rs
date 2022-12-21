use vector_config::configurable_component;

/// HTTP method.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// HTTP HEAD method.
    Head,

    /// HTTP GET method.
    Get,

    /// HTTP POST method.
    Post,

    /// HTTP Put method.
    Put,

    /// HTTP PATCH method.
    Patch,

    /// HTTP DELETE method.
    Delete,
}
