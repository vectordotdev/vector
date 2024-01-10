use http::Method;
use vector_lib::configurable::configurable_component;

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

impl From<HttpMethod> for Method {
    fn from(http_method: HttpMethod) -> Self {
        match http_method {
            HttpMethod::Head => Self::HEAD,
            HttpMethod::Get => Self::GET,
            HttpMethod::Post => Self::POST,
            HttpMethod::Put => Self::PUT,
            HttpMethod::Patch => Self::PATCH,
            HttpMethod::Delete => Self::DELETE,
        }
    }
}
