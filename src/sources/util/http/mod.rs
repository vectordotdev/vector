#[cfg(feature = "sources-utils-http-auth")]
mod auth;
#[cfg(feature = "sources-utils-http-encoding")]
mod encoding;
#[cfg(feature = "sources-utils-http-error")]
mod error;
#[cfg(any(
    feature = "sources-http_server",
    feature = "sources-opentelemetry",
    feature = "sources-utils-http-headers"
))]
mod headers;
mod method;
#[cfg(feature = "sources-utils-http-prelude")]
mod prelude;
#[cfg(any(
    feature = "sources-http_server",
    feature = "sources-heroku_logs",
    feature = "sources-utils-http-query"
))]
mod query;

#[cfg(feature = "sources-utils-http-auth")]
pub use auth::{HttpSourceAuth, HttpSourceAuthConfig};
#[cfg(feature = "sources-utils-http-encoding")]
pub use encoding::decode;
#[cfg(feature = "sources-utils-http-error")]
pub use error::ErrorMessage;
#[cfg(feature = "sources-utils-http-headers")]
pub use headers::add_headers;
pub use method::HttpMethod;
#[cfg(feature = "sources-utils-http-prelude")]
pub use prelude::HttpSource;
#[cfg(feature = "sources-utils-http-query")]
pub use query::add_query_parameters;
