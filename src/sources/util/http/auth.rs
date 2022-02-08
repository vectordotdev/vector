use std::convert::TryFrom;

use headers::{Authorization, HeaderMapExt};
use serde::{Deserialize, Serialize};
use warp::http::HeaderMap;

#[cfg(feature = "sources-utils-http-prelude")]
use super::error::ErrorMessage;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct HttpSourceAuthConfig {
    pub username: String,
    pub password: String,
}

impl TryFrom<Option<&HttpSourceAuthConfig>> for HttpSourceAuth {
    type Error = String;

    fn try_from(auth: Option<&HttpSourceAuthConfig>) -> Result<Self, Self::Error> {
        match auth {
            Some(auth) => {
                let mut headers = HeaderMap::new();
                headers.typed_insert(Authorization::basic(&auth.username, &auth.password));
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

#[derive(Debug, Clone)]
pub struct HttpSourceAuth {
    pub token: Option<String>,
}

#[cfg(feature = "sources-utils-http-prelude")]
impl HttpSourceAuth {
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
