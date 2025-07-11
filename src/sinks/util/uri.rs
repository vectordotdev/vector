use std::{fmt, str::FromStr};

use http::uri::{Authority, PathAndQuery, Scheme, Uri};
use percent_encoding::percent_decode_str;
use vector_lib::configurable::configurable_component;

use crate::http::Auth;

/// A wrapper for `http::Uri` that implements `Deserialize` and `Serialize`.
///
/// Authorization credentials, if they exist, will be removed from the URI and stored separately in `auth`.
#[configurable_component]
#[configurable(title = "The URI component of a request.", description = "")]
#[derive(Default, Debug, Clone)]
#[serde(try_from = "String", into = "String")]
pub struct UriSerde {
    pub uri: Uri,
    pub auth: Option<Auth>,
}

impl UriSerde {
    /// `Uri` supports incomplete URIs such as "/test", "example.com", etc.
    /// This function fills in empty scheme with HTTP,
    /// and empty authority with "127.0.0.1".
    pub fn with_default_parts(&self) -> Self {
        let mut parts = self.uri.clone().into_parts();
        if parts.scheme.is_none() {
            parts.scheme = Some(Scheme::HTTP);
        }
        if parts.authority.is_none() {
            parts.authority = Some(Authority::from_static("127.0.0.1"));
        }
        if parts.path_and_query.is_none() {
            // just an empty `path_and_query`,
            // but `from_parts` will fail without this.
            parts.path_and_query = Some(PathAndQuery::from_static(""));
        }
        let uri = Uri::from_parts(parts).expect("invalid parts");
        Self {
            uri,
            auth: self.auth.clone(),
        }
    }

    /// Creates a new instance of `UriSerde` by appending a path to the existing one.
    pub fn append_path(&self, path: &str) -> crate::Result<Self> {
        let uri = self.uri.to_string();
        let self_path = uri.trim_end_matches('/');
        let other_path = path.trim_start_matches('/');
        let path = format!("{}/{}", self_path, other_path);
        let uri = path.parse::<Uri>()?;
        Ok(Self {
            uri,
            auth: self.auth.clone(),
        })
    }

    #[allow(clippy::missing_const_for_fn)] // constant functions cannot evaluate destructors
    pub fn with_auth(mut self, auth: Option<Auth>) -> Self {
        self.auth = auth;
        self
    }
}

impl TryFrom<String> for UriSerde {
    type Error = <Uri as FromStr>::Err;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().parse()
    }
}

impl From<UriSerde> for String {
    fn from(uri: UriSerde) -> Self {
        uri.to_string()
    }
}

impl fmt::Display for UriSerde {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.uri.authority(), &self.auth) {
            (Some(authority), Some(Auth::Basic { user, password })) => {
                let authority = format!("{}:{}@{}", user, password, authority);
                let authority =
                    Authority::from_maybe_shared(authority).map_err(|_| std::fmt::Error)?;
                let mut parts = self.uri.clone().into_parts();
                parts.authority = Some(authority);
                Uri::from_parts(parts).unwrap().fmt(f)
            }
            _ => self.uri.fmt(f),
        }
    }
}

impl FromStr for UriSerde {
    type Err = <Uri as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Uri>().map(Into::into)
    }
}

impl From<Uri> for UriSerde {
    fn from(uri: Uri) -> Self {
        match uri.authority() {
            None => Self { uri, auth: None },
            Some(authority) => {
                let (authority, auth) = get_basic_auth(authority);

                let mut parts = uri.into_parts();
                parts.authority = Some(authority);
                let uri = Uri::from_parts(parts).unwrap();

                Self { uri, auth }
            }
        }
    }
}

fn get_basic_auth(authority: &Authority) -> (Authority, Option<Auth>) {
    // We get a valid `Authority` as input, therefore cannot fail here.
    let mut url = url::Url::parse(&format!("http://{}", authority)).expect("invalid authority");

    let user = url.username();
    if !user.is_empty() {
        let user = percent_decode_str(user).decode_utf8_lossy().into_owned();

        let password = url.password().unwrap_or("");
        let password = percent_decode_str(password)
            .decode_utf8_lossy()
            .into_owned();

        // These methods have the same failure condition as `username`,
        // because we have a non-empty username, they cannot fail here.
        url.set_username("").expect("unexpected empty authority");
        url.set_password(None).expect("unexpected empty authority");

        // We get a valid `Authority` as input, therefore cannot fail here.
        let authority = Uri::from_maybe_shared(String::from(url))
            .expect("invalid url")
            .authority()
            .expect("unexpected empty authority")
            .clone();

        (
            authority,
            Some(Auth::Basic {
                user,
                password: password.into(),
            }),
        )
    } else {
        (authority.clone(), None)
    }
}

/// Simplify the URI into a protocol and endpoint by removing the
/// "query" portion of the `path_and_query`.
pub fn protocol_endpoint(uri: Uri) -> (String, String) {
    let mut parts = uri.into_parts();

    // Drop any username and password
    parts.authority = parts.authority.map(|auth| {
        let host = auth.host();
        match auth.port() {
            None => host.to_string(),
            Some(port) => format!("{}:{}", host, port),
        }
        .parse()
        .unwrap_or_else(|_| unreachable!())
    });

    // Drop the query and fragment
    parts.path_and_query = parts.path_and_query.map(|pq| {
        pq.path()
            .parse::<PathAndQuery>()
            .unwrap_or_else(|_| unreachable!())
    });

    (
        parts.scheme.clone().unwrap_or(Scheme::HTTP).as_str().into(),
        Uri::from_parts(parts)
            .unwrap_or_else(|_| unreachable!())
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_parse(input: &str, expected_uri: &'static str, expected_auth: Option<(&str, &str)>) {
        let UriSerde { uri, auth } = input.parse().unwrap();
        assert_eq!(uri, Uri::from_static(expected_uri));
        assert_eq!(
            auth,
            expected_auth.map(|(user, password)| {
                Auth::Basic {
                    user: user.to_owned(),
                    password: password.to_owned().into(),
                }
            })
        );
    }

    #[test]
    fn parse_endpoint() {
        test_parse(
            "http://user:pass@example.com/test",
            "http://example.com/test",
            Some(("user", "pass")),
        );

        test_parse("localhost:8080", "localhost:8080", None);

        test_parse("/api/test", "/api/test", None);

        test_parse(
            "http://user:pass;@example.com",
            "http://example.com",
            Some(("user", "pass;")),
        );

        test_parse(
            "user:pass@example.com",
            "example.com",
            Some(("user", "pass")),
        );

        test_parse("user@example.com", "example.com", Some(("user", "")));
    }

    #[test]
    fn protocol_endpoint_parses_urls() {
        let parse = |uri: &str| protocol_endpoint(uri.parse().unwrap());

        assert_eq!(
            parse("http://example.com/"),
            ("http".into(), "http://example.com/".into())
        );
        assert_eq!(
            parse("https://user:pass@example.org:123/path?query"),
            ("https".into(), "https://example.org:123/path".into())
        );
        assert_eq!(
            parse("gopher://example.net:123/path?query#frag,emt"),
            ("gopher".into(), "gopher://example.net:123/path".into())
        );
    }
}
