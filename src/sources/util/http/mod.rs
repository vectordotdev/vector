#[cfg(feature = "sources-utils-http-auth")]
mod auth;
#[cfg(feature = "sources-utils-http-encoding")]
mod encoding;
#[cfg(feature = "sources-utils-http-error")]
mod error;
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
pub use method::HttpMethod;
#[cfg(feature = "sources-utils-http-prelude")]
pub use prelude::HttpSource;
#[cfg(feature = "sources-utils-http-query")]
pub use query::add_query_parameters;
