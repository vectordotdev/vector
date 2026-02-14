//! Common HTTP code. Similar to [`crate::http`], but based on [`http`] 1.0 crate.
#![allow(missing_docs)]

use headers_04::{Authorization, HeaderMapExt};
use http_1::{HeaderMap, Request, header::HeaderValue, request::Builder};
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

#[cfg(feature = "aws-core")]
use crate::aws::AwsAuthentication;

/// Configuration of the authentication strategy for HTTP requests.
///
/// HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
/// HTTP header without any additional encryption beyond what is provided by the transport itself.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
pub enum Auth {
    /// Basic authentication.
    ///
    /// The username and password are concatenated and encoded using [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    Basic {
        /// The basic authentication username.
        #[configurable(metadata(docs::examples = "${USERNAME}"))]
        #[configurable(metadata(docs::examples = "username"))]
        user: String,

        /// The basic authentication password.
        #[configurable(metadata(docs::examples = "${PASSWORD}"))]
        #[configurable(metadata(docs::examples = "password"))]
        password: SensitiveString,
    },

    /// Bearer authentication.
    ///
    /// The bearer token value (OAuth2, JWT, etc.) is passed as-is.
    Bearer {
        /// The bearer authentication token.
        token: SensitiveString,
    },

    #[cfg(feature = "aws-core")]
    /// AWS authentication.
    Aws {
        /// The AWS authentication configuration.
        auth: AwsAuthentication,

        /// The AWS service name to use for signing.
        service: String,
    },

    /// Custom Authorization Header Value, will be inserted into the headers as `Authorization: < value >`
    Custom {
        /// Custom string value of the Authorization header
        #[configurable(metadata(docs::examples = "${AUTH_HEADER_VALUE}"))]
        #[configurable(metadata(docs::examples = "CUSTOM_PREFIX ${TOKEN}"))]
        value: String,
    },
}

pub trait MaybeAuth: Sized {
    fn choose_one(&self, other: &Self) -> crate::Result<Self>;
}

impl MaybeAuth for Option<Auth> {
    fn choose_one(&self, other: &Self) -> crate::Result<Self> {
        if self.is_some() && other.is_some() {
            Err("Two authorization credentials was provided.".into())
        } else {
            Ok(self.clone().or_else(|| other.clone()))
        }
    }
}

impl Auth {
    pub fn apply<B>(&self, req: &mut Request<B>) {
        self.apply_headers_map(req.headers_mut())
    }

    pub fn apply_builder(&self, mut builder: Builder) -> Builder {
        if let Some(map) = builder.headers_mut() {
            self.apply_headers_map(map)
        }
        builder
    }

    pub fn apply_headers_map(&self, map: &mut HeaderMap) {
        match &self {
            Auth::Basic { user, password } => {
                let auth = Authorization::basic(user.as_str(), password.inner());
                map.typed_insert(auth);
            }
            Auth::Bearer { token } => match Authorization::bearer(token.inner()) {
                Ok(auth) => map.typed_insert(auth),
                Err(error) => error!(message = "Invalid bearer token.", token = %token, %error),
            },
            Auth::Custom { value } => {
                // The value contains just the value for the Authorization header
                // Expected format: "SSWS token123" or "Bearer token123", etc.
                match HeaderValue::from_str(value) {
                    Ok(header_val) => {
                        map.insert(http_1::header::AUTHORIZATION, header_val);
                    }
                    Err(error) => {
                        error!(message = "Invalid custom auth header value.", value = %value, %error)
                    }
                }
            }
            #[cfg(feature = "aws-core")]
            _ => {}
        }
    }
}
