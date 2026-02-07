//! Common module between modules that use HTTP
#[cfg(all(
    feature = "sources-utils-http-auth",
    feature = "sources-utils-http-error"
))]
pub mod server_auth;

#[cfg(all(
    feature = "sources-utils-http-auth",
    feature = "sources-utils-http-error"
))]
pub mod server_auth_http1;

#[cfg(feature = "sources-utils-http-error")]
mod error;

#[cfg(feature = "sources-utils-http-error")]
pub use error::ErrorMessage;
