use std::convert::TryFrom;

use headers::{Authorization, HeaderMapExt};
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;
use warp::http::HeaderMap;

#[cfg(any(
    feature = "sources-utils-http-prelude",
    feature = "sources-utils-http-auth"
))]
use super::error::ErrorMessage;

/// HTTP Basic authentication configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct HttpSourceAuthConfig {
    /// The username for basic authentication.
    #[configurable(metadata(docs::examples = "AzureDiamond"))]
    #[configurable(metadata(docs::examples = "admin"))]
    pub username: String,

    /// The password for basic authentication.
    #[configurable(metadata(docs::examples = "hunter2"))]
    #[configurable(metadata(docs::examples = "${PASSWORD}"))]
    pub password: SensitiveString,
}

impl TryFrom<Option<&HttpSourceAuthConfig>> for HttpSourceAuth {
    type Error = String;

    fn try_from(auth: Option<&HttpSourceAuthConfig>) -> Result<Self, Self::Error> {
        match auth {
            Some(auth) => {
                let mut headers = HeaderMap::new();
                headers.typed_insert(Authorization::basic(
                    auth.username.as_str(),
                    auth.password.inner(),
                ));
                match headers.get("authorization") {
                    Some(value) => {
                        let token = value
                            .to_str()
                            .map_err(|error| format!("Failed stringify HeaderValue: {:?}", error))?
                            .to_owned();
                        Ok(HttpSourceAuth { token: Some(token) })
                    }
                    None => Err("Authorization headers wasn't generated".to_owned()),
                }
            }
            None => Ok(HttpSourceAuth { token: None }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HttpSourceAuth {
    #[allow(unused)] // triggered by check-component-features
    pub(self) token: Option<String>,
}

impl HttpSourceAuth {
    #[allow(unused)] // triggered by check-component-features
    pub fn is_valid(&self, header: &Option<String>) -> Result<(), ErrorMessage> {
        use warp::http::StatusCode;

        match (&self.token, header) {
            (Some(token1), Some(token2)) => {
                if token1 == token2 {
                    Ok(())
                } else {
                    Err(ErrorMessage::new(
                        StatusCode::UNAUTHORIZED,
                        "Invalid username/password".to_owned(),
                    ))
                }
            }
            (Some(_), None) => Err(ErrorMessage::new(
                StatusCode::UNAUTHORIZED,
                "No authorization header".to_owned(),
            )),
            (None, _) => Ok(()),
        }
    }
}
